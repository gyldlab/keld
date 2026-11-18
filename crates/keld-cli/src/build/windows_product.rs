//! Ordinary Windows local-test product route. No installer, key creation or
//! trust activation. Project authoring remains the same four-file contract.
use std::fs::File;
use std::io::Read as _;
use std::path::{Component,Path,PathBuf};
use serde::Deserialize;
use super::{BuildError,config,local_archive_command,local_images,local_source,windows_inputs};
const MAX_INPUT:u64=64*1024;
fn refuse(e:impl std::fmt::Display)->BuildError {BuildError::input("Windows local-test build",e)}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ToolPin {path:PathBuf,sha256:String}
#[derive(Deserialize)]
#[serde(rename_all="kebab-case")]
enum CacheMode {Snapshot,MutableDeveloper}
/// Explicit ephemeral transaction input, never another persistent project file.
/// Public key and existing certificate references only; no secret is parsed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields,rename_all="camelCase")]
struct TestSession {
    schema:String,expected_source_commit:String,git:PathBuf,cargo:PathBuf,rustc:PathBuf,cargo_home:PathBuf,
    output_nonce:String,sign_tool:ToolPin,certificate_thumbprint:String,
    publisher_scope:String,release_public_key:PathBuf,published_at:String,cache_mode:CacheMode,
}
fn parse_session(bytes:&[u8])->Result<TestSession,BuildError> {
    if bytes.is_empty()||bytes.len() as u64>MAX_INPUT {return Err(refuse("session exceeds bounds"));}
    let session:TestSession=serde_json::from_slice(bytes).map_err(refuse)?;
    if session.schema!="keld.windows-local-test-session/v1" || session.output_nonce.is_empty() || session.output_nonce.len()>64
        || !session.output_nonce.bytes().all(|b|b.is_ascii_alphanumeric()||matches!(b,b'-'|b'_')) {
        return Err(refuse("explicit test-session schema and bounded output nonce required"));
    }
    for path in [&session.git,&session.cargo,&session.rustc,&session.cargo_home,&session.sign_tool.path,&session.release_public_key] {
        if !path.is_absolute() {return Err(refuse("session tools and public-key locators must be absolute"));}
    }
    Ok(session)
}
fn require_source_commit(expected:&str,recipe:&str)->Result<(),BuildError> {
    if expected!=recipe {return Err(refuse("recipe source commit differs from explicitly selected integrated Host source"));}Ok(())
}
fn resolve(root:&Path,path:&Path)->Result<PathBuf,BuildError> {
    if path.as_os_str().is_empty() || path.components().any(|c|matches!(c,Component::ParentDir|Component::CurDir)) {
        return Err(refuse("noncanonical build locator"));
    }
    if path.is_absolute() {Ok(path.to_owned())} else if path.components().any(|c|matches!(c,Component::Prefix(_)|Component::RootDir)) {
        Err(refuse("rooted-relative build locator"))
    } else {Ok(root.join(path))}
}
fn read(pin:&mut local_source::PinnedFile)->Result<Vec<u8>,BuildError> {
    let mut bytes=Vec::new();(&mut pin.file).take(MAX_INPUT+1).read_to_end(&mut bytes).map_err(refuse)?;
    if bytes.len() as u64>MAX_INPUT {return Err(refuse("input grew beyond bound"));}Ok(bytes)
}
/// Retains the actual project/config/session namespace throughout the same
/// source/image/archive transaction. Config cannot choose a command or installer.
pub(super) fn run(session_path:&Path)->Result<(),BuildError> {
    if !session_path.is_absolute() {return Err(refuse("--local-test-session requires an absolute ephemeral input locator"));}
    let root=std::env::current_dir().map_err(refuse)?;
    let _project=windows_inputs::pin_input_root(&root)?;
    let mut build=local_source::pin_file(&root.join("keld.build.ts"),MAX_INPUT)?;
    let bytes=read(&mut build)?;
    let recipe=config::parse_windows_build(std::str::from_utf8(&bytes).map_err(refuse)?)?;
    let parent=session_path.parent().ok_or_else(||refuse("session input parent missing"))?;
    let _session_parents=windows_inputs::pin_input_root(parent)?;
    let mut session_pin=local_source::pin_file(session_path,MAX_INPUT)?;
    let session=parse_session(&read(&mut session_pin)?)?;
    require_source_commit(&session.expected_source_commit,&recipe.source_commit)?;
    let out_dir=resolve(&root,&recipe.out_dir)?;
    // Existing output root is admitted, not silently created/repaired with a new
    // policy. Same source owner creates only the fresh transaction child below it.
    let _output:Vec<File>=windows_inputs::pin_input_root(&out_dir)?;
    let source=local_source::Request {source:resolve(&root,&recipe.source_root)?,commit:recipe.source_commit,
        session:out_dir.join(format!("session-{}",session.output_nonce)),git:session.git,cargo:session.cargo,rustc:session.rustc,
        cargo_home:session.cargo_home,cache_mode:match session.cache_mode {CacheMode::Snapshot=>local_source::CacheMode::Snapshot,CacheMode::MutableDeveloper=>local_source::CacheMode::MutableDeveloper}};
    let images=local_images::ImageRequest {source,app_root:root.clone(),channel:recipe.channel,
        release_public_key:session.release_public_key,sign_tool:session.sign_tool.path,sign_tool_sha256:session.sign_tool.sha256,
        certificate_thumbprint:session.certificate_thumbprint,publisher_scope:session.publisher_scope};
    let compiler=local_archive_command::CompilerRequest {typescript_root:resolve(&root,&recipe.typescript_root)?,
        bun:resolve(&root,&recipe.bun.path)?,bun_sha256:recipe.bun.sha256,tsc:resolve(&root,&recipe.tsc.path)?,tsc_sha256:recipe.tsc.sha256};
    println!("Building Windows LOCAL TEST package with an existing test signer; not an official release or installation.");
    local_archive_command::run_product(images,compiler,&recipe.modules,&session.published_at)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn session()->serde_json::Value {serde_json::json!({"schema":"keld.windows-local-test-session/v1","expectedSourceCommit":"1".repeat(40),"git":"D:/git.exe","cargo":"D:/cargo.exe","rustc":"D:/rustc.exe","cargoHome":"D:/cache","outputNonce":"test-1","signTool":{"path":"D:/signtool.exe","sha256":"2".repeat(64)},"certificateThumbprint":"3".repeat(40),"publisherScope":"4".repeat(64),"releasePublicKey":"D:/public.raw","publishedAt":"2026-11-11T00:00:00Z","cacheMode":"mutable-developer"})}
    #[test] fn ephemeral_input_cannot_override_project_or_create_authority() {
        let value=session();assert!(parse_session(&serde_json::to_vec(&value).unwrap()).is_ok());
        for key in ["privateKey","command","install","source","modules","appId","createCertificate"] {
            let mut bad=value.clone();bad[key]=serde_json::json!("override");assert!(parse_session(&serde_json::to_vec(&bad).unwrap()).is_err());
        }
        for nonce in ["..","../other","a/b",""] {let mut bad=value.clone();bad["outputNonce"]=serde_json::json!(nonce);assert!(parse_session(&serde_json::to_vec(&bad).unwrap()).is_err());}
    }
    #[test] fn stale_host_commit_cannot_replace_selected_integrated_source() {
        require_source_commit(&"1".repeat(40),&"1".repeat(40)).unwrap();
        assert!(require_source_commit(&"1".repeat(40),&"2".repeat(40)).is_err());
    }
    #[test] fn relative_recipe_paths_have_one_project_root_without_traversal() {
        let root=Path::new("D:/app");assert_eq!(resolve(root,Path::new("out")).unwrap(),root.join("out"));
        assert!(resolve(root,Path::new("../out")).is_err());assert!(resolve(root,Path::new("./out")).is_err());
    }
}
