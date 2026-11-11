"""Read-only compiler-context preview. No source writer, subprocess or plan imports."""
import hashlib,json,sys,os,stat
from pathlib import Path
EXPECTED='9efb3330cf1365182b00aa4425f4dff7afb2d31e75e3626383668c7f862b4a6b'
HELPERS='fn reserved_transport(name: &str) -> bool {\n    name.rsplit(\'/\').next().is_some_and(|leaf|\n        leaf.eq_ignore_ascii_case("kipc-transport.ts") || leaf.eq_ignore_ascii_case("kipc-transport.js"))\n}\nfn read_pinned(source: &local_source::PinnedFile) -> Result<Vec<u8>, BuildError> {\n    let size = source.file.metadata().map_err(refusal)?.len();\n    if size > MAX_FILE { return Err(refusal("canonical transport bound")); }\n    let mut bytes = vec![0; usize::try_from(size).map_err(refusal)?];\n    let mut offset = 0;\n    while offset < bytes.len() {\n        let count = source.file.seek_read(&mut bytes[offset..], offset as u64).map_err(refusal)?;\n        if count == 0 { return Err(refusal("canonical transport truncated")); }\n        offset += count;\n    }\n    if source.file.seek_read(&mut [0], size).map_err(refusal)? != 0 { return Err(refusal("canonical transport grew")); }\n    Ok(bytes)\n}\nfn require_scaffold_bytes(actual: &[u8], canonical: &[u8], app_name: &str) -> Result<(), BuildError> {\n    let canonical = std::str::from_utf8(canonical).map_err(refusal)?;\n    // Reuse the existing create owner, including its release substitution and stamp.\n    let expected = crate::template::render_transport(canonical, app_name);\n    if actual != expected.as_bytes() { return Err(refusal("scaffold transport differs from retained compiler-source canonical transport")); }\n    Ok(())\n}\nfn verify_scaffold_transport(actual: &local_source::PinnedFile, canonical: &local_source::PinnedFile, app_name: &str) -> Result<(), BuildError> {\n    require_scaffold_bytes(&read_pinned(actual)?, &read_pinned(canonical)?, app_name)\n}\n\n'
CAPTURE='        let scaffold_transport = if entry.ends_with(".ts") {\n            let parent = root.join(entry).parent().ok_or_else(|| refusal("entry parent missing"))?.to_owned();\n            directories.extend(windows_inputs::pin_input_root(&parent)?);\n            let actual = local_source::pin_file(&parent.join("kipc-transport.ts"), MAX_FILE)?;\n            let canonical = sources.canonical_transport()?;\n            verify_scaffold_transport(&actual, canonical, app_name)?;\n            Some(actual)\n        } else { None };\n'
TESTS='#[cfg(test)]\nmod scaffold_source_tests {\n    use super::*;\n    #[test]\n    fn actual_canonical_transport_is_required_not_an_arbitrary_stamped_file() {\n        let source = "declare const KELD_KIPC_RELEASE: boolean | undefined;\\nexport const app = \'{{name}}\';\\n";\n        let rendered = crate::template::render_transport(source, "hello");\n        require_scaffold_bytes(rendered.as_bytes(), source.as_bytes(), "hello").unwrap();\n        assert!(require_scaffold_bytes(rendered.as_bytes(), source.as_bytes(), "other").is_err());\n        assert!(require_scaffold_bytes(b"export const arbitrary = true;", source.as_bytes(), "hello").is_err());\n        let changed = crate::template::restamp_transport("export const changed = true;\\n");\n        assert!(require_scaffold_bytes(changed.as_bytes(), source.as_bytes(), "hello").is_err());\n        assert!(require_scaffold_bytes(rendered.as_bytes(), &[0xff], "hello").is_err());\n    }\n    #[test]\n    fn reserved_source_names_are_case_insensitive_in_every_directory() {\n        for name in ["kipc-transport.ts", "src/KIPC-TRANSPORT.TS", "any/Kipc-Transport.Js"] {\n            assert!(reserved_transport(name));\n        }\n        assert!(!reserved_transport("src/echo.generated.ts"));\n    }\n}\n'

def replacement(text,old,new):
    if text.count(old)!=1:raise ValueError('missing_or_duplicate_anchor')
    return text.replace(old,new,1)

def transform_text(text):
    suffix_anchor="fn bun_config_argument("
    if text.count(suffix_anchor)!=1:raise ValueError("native_config_owner_missing")
    preserved_suffix=text[text.index(suffix_anchor):]
    operations=[
      ('pub(super) struct CapturedModules {', HELPERS+'pub(super) struct CapturedModules {\n    entry: String,\n    _scaffold_transport: Option<local_source::PinnedFile>,'),
      ('    pub(super) fn open(root: &Path, names: &[String]) -> Result<Self, BuildError> {',
       "    pub(super) fn open(root: &Path, names: &[String], entry: &str, app_name: &str, sources: &local_source::ObservedCompilerSources<'_>) -> Result<Self, BuildError> {"),
      ('!names.iter().any(|n| n == "main.js")','!names.iter().any(|n| n == entry)'),
      ('"expected root main.js and at most4096 explicit modules"','"expected declared entry and at most4096 explicit modules"'),
      ('name.strip_suffix(".js").is_none() || name == "kipc-transport.js" || name.len() > 100',
       '!(name.ends_with(".js") || name.ends_with(".ts")) || reserved_transport(name) || name.len() > 100'),
      ('        let mut directories = windows_inputs::pin_input_root(root)?;\n',
       '        let mut directories = windows_inputs::pin_input_root(root)?;\n'+CAPTURE),
      ('        Ok(Self {\n','        Ok(Self {\n            entry: entry.to_owned(),\n            _scaffold_transport: scaffold_transport,\n'),
      ('        output.write_all(b"KELDCIN2").map_err(refusal)?;',
       '        output.write_all(b"KELDCIN3").map_err(refusal)?;\n        output.write_all(&u32::try_from(self.entry.len()).map_err(refusal)?.to_le_bytes()).map_err(refusal)?;\n        output.write_all(self.entry.as_bytes()).map_err(refusal)?;'),
    ]
    for old,new in operations:text=replacement(text,old,new)
    if 'mod scaffold_source_tests' in text:raise ValueError('tests_already_present')
    if text[text.index(suffix_anchor):]!=preserved_suffix:raise ValueError('native_compile_or_existing_tests_changed')
    return text+'\n'+TESTS

def preview(raw):
    if hashlib.sha256(raw).hexdigest()!=EXPECTED:raise ValueError('baseline_mismatch')
    text=raw.decode('utf-8');after=transform_text(text).encode()
    return {'before_sha256':EXPECTED,'before_bytes':len(raw),'after_sha256':hashlib.sha256(after).hexdigest(),'after_bytes':len(after),'source_written':False,'operation_count':8}

def main():
    if len(sys.argv)!=2:raise ValueError('one_absolute_repository_root')
    root=Path(sys.argv[1])
    if not root.is_absolute():raise ValueError('absolute_root')
    path=root/'crates/keld-cli/src/build/windows_compiler.rs'
    for p in [path,*path.parents]:
        st=p.lstat()
        if p.is_symlink() or getattr(st,'st_file_attributes',0)&0x400:raise ValueError('alias')
    if not stat.S_ISREG(path.lstat().st_mode):raise ValueError('regular_source_required')
    descriptor=os.open(path,os.O_RDONLY|getattr(os,'O_BINARY',0)|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
    with os.fdopen(descriptor,'rb') as stream:
        if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):raise ValueError('regular_source_handle_required')
        raw=stream.read(128*1024+1)
        if len(raw)>128*1024:raise ValueError('source_bound')
        result=preview(raw)
        stream.seek(0)
        if stream.read(128*1024+1)!=raw:raise ValueError('changed')
    print(json.dumps(result))
if __name__=='__main__':
    try:main()
    except Exception:
        print('{"error":"compiler_preview_refused","source_written":false}',file=sys.stderr);raise SystemExit(2)
