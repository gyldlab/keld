//! Immutable evidence URI admission: pin validation and public-network host rejection.
//! This is an internal trust policy; no DNS lookup or remote request is performed.

use super::{EvidenceError, parse_digest};
use std::net::{Ipv4Addr, Ipv6Addr};

pub(super) fn parse_evidence_uri(uri: &str) -> Result<String, EvidenceError> {
    let trimmed = uri.trim();
    if trimmed.is_empty() {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: "empty evidence_uri".to_owned(),
        });
    }
    if is_lead_not_location(trimmed) {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{trimmed}` is a sandbox path or opaque turn citation"),
        });
    }
    if trimmed.starts_with("sha256:") {
        parse_digest(trimmed)?;
        return Ok(trimmed.to_owned());
    }
    if let Some(rest) = trimmed.strip_prefix("https://") {
        // Preserve strict authority syntax (including empty userinfo/ports) before
        // WHATWG parsing, which otherwise repairs some malformed inputs.
        let authority_and_path = rest
            .split_once(['?', '#'])
            .map_or(rest, |(prefix, _)| prefix);
        parse_https_host_and_path(authority_and_path, trimmed)?;
        if trimmed.bytes().any(|b| b.is_ascii_control()) || authority_and_path.contains('\\') {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` contains ambiguous URL characters"),
            });
        }
        let url = parse_credential_free_https_url(trimmed)?;
        let host = match url.host() {
            Some(url::Host::Domain(host)) => host.to_owned(),
            Some(url::Host::Ipv4(host)) => host.to_string(),
            Some(url::Host::Ipv6(host)) => host.to_string(),
            None => String::new(),
        };
        // Trailing FQDN dots are the same host (RFC 1034); compare once.
        let host = host.trim_end_matches('.').to_ascii_lowercase();
        let (path, nested_path) = decoded_policy_paths(url.path(), trimmed)?;
        if host_is_forbidden(&host) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a public https location"),
            });
        }
        if host.parse::<Ipv4Addr>().is_err()
            && host.parse::<Ipv6Addr>().is_err()
            && !host.contains('.')
        {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a resolvable https URL with a path"),
            });
        }
        if path.is_empty() || path == "/" {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{trimmed}` is not a resolvable https URL with a path"),
            });
        }
        if https_path_is_live_mutable(&host, &path)
            || https_path_is_live_mutable(&host, &nested_path)
        {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has a live branch/tag path; \
                     a mutable URL is not an immutable pin"
                ),
            });
        }
        if !path_has_git_object_id(&path) || !path_has_git_object_id(&nested_path) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has no git object id in the path; \
                     a live-mutable URL is not an immutable pin"
                ),
            });
        }
        return Ok(url.into());
    }
    Err(EvidenceError::NonNormativeEvidence {
        detail: format!("`{trimmed}` is not https:// or sha256:"),
    })
}

fn parse_credential_free_https_url(uri: &str) -> Result<url::Url, EvidenceError> {
    let url = url::Url::parse(uri).map_err(|_| EvidenceError::NonNormativeEvidence {
        detail: format!("`{uri}` is not a public https location"),
    })?;
    if !url.username().is_empty() || url.password().is_some() {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: "HTTPS evidence locations must not contain credentials".to_owned(),
        });
    }
    Ok(url)
}

/// Check both the one-decode URL path and its deeply decoded safety view.
/// A second decoding pass must never create the only accepted git pin.
/// The serialized evidence URL is not rewritten here.
fn decoded_policy_paths(path: &str, original: &str) -> Result<(String, String), EvidenceError> {
    const POLICY_ESCAPES: &percent_encoding::AsciiSet =
        &percent_encoding::CONTROLS.add(b'%').add(b'?').add(b'#');
    let mut segments = Vec::new();
    let mut nested_segments = Vec::new();
    for segment in path.split('/') {
        let malformed_escape = segment
            .as_bytes()
            .iter()
            .enumerate()
            .any(|(offset, &byte)| {
                byte == b'%'
                    && !segment
                        .as_bytes()
                        .get(offset + 1..offset + 3)
                        .is_some_and(|pair| pair.iter().all(u8::is_ascii_hexdigit))
            });
        // Reduce percent triplets to a fixed point in one linear pass. A
        // decoded literal "%a1" filename is safe, but a multi-encoded
        // slash, backslash, control or dot-only segment is not.
        let once_decoded: Vec<_> = percent_encoding::percent_decode_str(segment).collect();
        let decoded = fully_decode_path_segment(segment);
        if malformed_escape
            || matches!(once_decoded.as_slice(), b"." | b"..")
            || once_decoded
                .iter()
                .any(|&b| matches!(b, b'/' | b'\\') || b.is_ascii_control())
            || matches!(decoded.as_slice(), b"." | b"..")
            || decoded
                .iter()
                .any(|&b| matches!(b, b'/' | b'\\') || b.is_ascii_control())
        {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!("`{original}` has an ambiguous encoded path"),
            });
        }
        // Preserve non-ASCII filename bytes and escaped punctuation in the
        // policy view. Pin and route tokens use ASCII; the returned URL is unchanged.
        segments.push(percent_encoding::percent_encode(&once_decoded, POLICY_ESCAPES).to_string());
        nested_segments
            .push(percent_encoding::percent_encode(&decoded, POLICY_ESCAPES).to_string());
    }
    Ok((segments.join("/"), nested_segments.join("/")))
}

fn fully_decode_path_segment(segment: &str) -> Vec<u8> {
    let mut decoded = Vec::with_capacity(segment.len());
    for byte in segment.bytes() {
        decoded.push(byte);
        while let Some(start) = decoded.len().checked_sub(3) {
            if decoded[start] != b'%' {
                break;
            }
            let (Some(hi), Some(lo)) = (
                hex_nibble(decoded[start + 1]),
                hex_nibble(decoded[start + 2]),
            ) else {
                break;
            };
            decoded.truncate(start);
            decoded.push((hi << 4) | lo);
        }
    }
    decoded
}

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Turn citations and local temp *paths*. Not a substring search on https URLs.
fn is_lead_not_location(uri: &str) -> bool {
    let lower = uri.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("sha256:") {
        return false;
    }
    lower.starts_with("turn")
        || lower.starts_with("file:")
        || lower.starts_with("sandbox:")
        || lower.starts_with("/tmp/")
        || lower.starts_with("/private/tmp/")
        || lower.starts_with("/var/folders/")
        || lower.starts_with("\\temp\\")
        || lower.contains(":\\users\\") && lower.contains("\\appdata\\local\\temp")
}

fn parse_https_host_and_path(
    authority_and_path: &str,
    original: &str,
) -> Result<(String, String), EvidenceError> {
    if authority_and_path.is_empty() || authority_has_userinfo(authority_and_path) {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a public https location"),
        });
    }
    if authority_and_path.starts_with('[') {
        let end =
            authority_and_path
                .find(']')
                .ok_or_else(|| EvidenceError::NonNormativeEvidence {
                    detail: format!("`{original}` is not a resolvable https URL with a path"),
                })?;
        let host = authority_and_path[1..end].to_owned();
        let after = &authority_and_path[end + 1..];
        let path = match after.strip_prefix(':') {
            Some(port_and_path) => {
                let path_at =
                    port_and_path
                        .find('/')
                        .ok_or_else(|| EvidenceError::NonNormativeEvidence {
                            detail: format!(
                                "`{original}` is not a resolvable https URL with a path"
                            ),
                        })?;
                let port = &port_and_path[..path_at];
                if !https_port_is_valid(port) {
                    return Err(EvidenceError::NonNormativeEvidence {
                        detail: format!("`{original}` is not a public https location"),
                    });
                }
                port_and_path[path_at..].to_owned()
            }
            None => {
                if after.is_empty() {
                    String::new()
                } else if let Some(path) = after.strip_prefix('/') {
                    format!("/{path}")
                } else {
                    return Err(EvidenceError::NonNormativeEvidence {
                        detail: format!("`{original}` is not a resolvable https URL with a path"),
                    });
                }
            }
        };
        return Ok((host, path));
    }
    if authority_and_path.matches(':').count() > 1 {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a resolvable https URL with a path"),
        });
    }
    let slash = authority_and_path.find('/');
    let hostport = slash.map_or(authority_and_path, |i| &authority_and_path[..i]);
    if hostport.is_empty() {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a public https location"),
        });
    }
    let host = match hostport.rsplit_once(':') {
        Some((h, port)) => {
            if h.is_empty() || !https_port_is_valid(port) {
                return Err(EvidenceError::NonNormativeEvidence {
                    detail: format!("`{original}` is not a public https location"),
                });
            }
            h.to_owned()
        }
        None => hostport.to_owned(),
    };
    let path = slash.map_or(String::new(), |i| authority_and_path[i..].to_owned());
    Ok((host, path))
}

fn authority_has_userinfo(rest: &str) -> bool {
    let mut in_brackets = false;
    for byte in rest.bytes() {
        match byte {
            b'[' => in_brackets = true,
            b']' => in_brackets = false,
            b'/' if !in_brackets => return false,
            b'@' if !in_brackets => return true,
            _ => {}
        }
    }
    false
}

/// Decimal port that fits in `u16`. Non-numeric and `65536` are not public https.
fn https_port_is_valid(port: &str) -> bool {
    !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok()
}

fn host_is_forbidden(host: &str) -> bool {
    // Browsers treat a trailing FQDN dot as the same host; `Ipv4Addr` does not.
    // URL parsing has already canonicalized literal IP addresses. No DNS
    // lookup is performed; DNS-to-private hosts remain a residual risk.
    let h = host.to_ascii_lowercase();
    let h = h.trim_end_matches('.');
    if h.is_empty() || h == "localhost" || h.ends_with(".localhost") {
        return true;
    }
    if let Ok(v4) = h.parse::<Ipv4Addr>() {
        return ipv4_is_non_public(v4);
    }
    if let Ok(v6) = h.parse::<Ipv6Addr>() {
        return ipv6_is_non_public(v6);
    }
    false
}

fn ipv4_is_non_public(v4: Ipv4Addr) -> bool {
    let [a, b, c, d] = v4.octets();
    // IANA IPv4 Special-Purpose Address Registry: deny non-globally reachable
    // blocks, plus multicast (not unicast evidence) and reserved space.
    // 192.0.0.9 and .10 are the registry's globally reachable exceptions.
    v4.is_loopback()
        || v4.is_private()
        || v4.is_link_local()
        || ipv4_is_cgnat(v4)
        || matches!(
            [a, b, c],
            [0 | 224..=255, _, _]
                | [192, 0, 2]
                | [192, 88, 99]
                | [198, 18 | 19, _]
                | [198, 51, 100]
                | [203, 0, 113]
        )
        || ([a, b, c] == [192, 0, 0] && !matches!(d, 9 | 10))
}

/// RFC 6598 shared address space `100.64.0.0/10` (CGNAT).
/// `Ipv4Addr::is_shared` is still unstable (`feature(ip)`).
fn ipv4_is_cgnat(v4: Ipv4Addr) -> bool {
    let [a, b, ..] = v4.octets();
    a == 100 && (64..128).contains(&b)
}

fn ipv6_is_non_public(v6: Ipv6Addr) -> bool {
    if v6.is_loopback() || v6.is_unspecified() || v6.is_unique_local() || v6.is_unicast_link_local()
    {
        return true;
    }
    if ipv6_is_nat64_well_known(v6) || ipv6_is_6to4(v6) {
        return true;
    }
    ipv6_embedded_ipv4(v6).is_some_and(ipv4_is_non_public)
}

/// RFC 6052 well-known NAT64 prefix `64:ff9b::/96`.
fn ipv6_is_nat64_well_known(v6: Ipv6Addr) -> bool {
    let o = v6.octets();
    o[..12] == [0x00, 0x64, 0xff, 0x9b, 0, 0, 0, 0, 0, 0, 0, 0]
}

/// RFC 3056 6to4 prefix `2002::/16`.
fn ipv6_is_6to4(v6: Ipv6Addr) -> bool {
    let o = v6.octets();
    o[0] == 0x20 && o[1] == 0x02
}

/// IPv4-mapped (`::ffff:a.b.c.d`), deprecated IPv4-compatible (`::a.b.c.d`),
/// and IPv4-translated (`::ffff:0:a.b.c.d`, RFC 2765). `::` / `::1` are
/// classified before this runs.
fn ipv6_embedded_ipv4(v6: Ipv6Addr) -> Option<Ipv4Addr> {
    if let Some(v4) = v6.to_ipv4_mapped() {
        return Some(v4);
    }
    let o = v6.octets();
    let tail = Ipv4Addr::new(o[12], o[13], o[14], o[15]);
    let ipv4_compatible = o[..12] == [0; 12];
    let ipv4_translated =
        o[..8] == [0; 8] && o[8] == 0xff && o[9] == 0xff && o[10] == 0 && o[11] == 0;
    (ipv4_compatible || ipv4_translated).then_some(tail)
}

fn path_has_git_object_id(path: &str) -> bool {
    path.split('/').any(is_git_object_id)
}

/// `/blob/<branch>/`, `/tree/<branch>/`, `/raw/<branch>/`, and GitHub raw CDN
/// `/{owner}/{repo}/{branch}/…` stay live-mutable even when a later segment is
/// 40- or 64-hex. Commit-pinned `/blob/<object-id>/` is not this case.
fn https_path_is_live_mutable(host: &str, path: &str) -> bool {
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if host == "raw.githubusercontent.com" {
        return segs
            .get(2)
            .is_some_and(|git_ref| !is_git_object_id(git_ref));
    }
    // Once a known provider's route has an immutable ref, later "blob",
    // "tree", or "raw" components are ordinary directory names.
    if has_pinned_provider_ref(host, &segs) {
        return false;
    }
    // A later SHA-shaped filename or workspace name cannot pin Bitbucket's
    // source route. The provider ref itself must be immutable.
    if host == "bitbucket.org"
        && segs
            .get(2)
            .is_some_and(|segment| segment.eq_ignore_ascii_case("src"))
    {
        return true;
    }
    // On an unknown public host, a git object id establishes the immutable
    // location. Route-like directory names after it are file-path data.
    // Named providers keep their existing route-position checks above.
    let generic_host = !matches!(host, "github.com" | "gitlab.com" | "bitbucket.org");
    // A SHA-looking project or group name is not proof that a later
    // GitLab/GitHub-style provider route has a pinned revision.
    if generic_host && generic_host_has_live_provider_ref(&segs) {
        return true;
    }
    path_has_live_git_ref(path, generic_host)
}

fn generic_host_has_live_provider_ref(segments: &[&str]) -> bool {
    let is_route = |segment: &str| {
        matches!(
            segment.to_ascii_lowercase().as_str(),
            "commit" | "commits" | "blob" | "tree" | "raw"
        )
    };
    // GitLab self-hosted instances support nested groups before /-/blob/<ref>.
    // Only the first provider route is authoritative; later route-like names
    // in a pinned file path remain directory data.
    if let Some(route) = segments
        .windows(3)
        .find(|part| part[0] == "-" && is_route(part[1]))
    {
        return !is_git_object_id(route[2]);
    }
    // Gitea uses /<owner>/<repo>/src/branch/<mutable-ref> and
    // /src/commit/<immutable-sha>. Its /commits/branch/<ref> route is
    // also mutable, even when a project name resembles a SHA.
    if segments
        .get(2)
        .is_some_and(|segment| segment.eq_ignore_ascii_case("commits"))
    {
        let ref_kind = segments.get(3).copied().unwrap_or_default();
        return !is_git_object_id(ref_kind);
    }
    // A Bitbucket-like /src/<ref> must
    // also pin the ref at its actual position.
    if segments
        .get(2)
        .is_some_and(|segment| segment.eq_ignore_ascii_case("src"))
    {
        let ref_kind = segments.get(3).copied().unwrap_or_default();
        return match ref_kind.to_ascii_lowercase().as_str() {
            "branch" | "tag" => true,
            "commit" => !segments.get(4).is_some_and(|pin| is_git_object_id(pin)),
            _ => !is_git_object_id(ref_kind),
        };
    }
    // A GitHub/Gitea-style route follows /<owner>/<repo>/, even when
    // the owner or repository slug itself looks like a git object id.
    segments.get(2).is_some_and(|segment| is_route(segment))
        && !segments
            .get(3)
            .is_some_and(|segment| is_git_object_id(segment))
}

fn has_pinned_provider_ref(host: &str, segments: &[&str]) -> bool {
    let is_git_route = |segment: &str| {
        matches!(
            segment.to_ascii_lowercase().as_str(),
            "commit" | "commits" | "blob" | "tree" | "raw"
        )
    };
    match host {
        "github.com" => {
            segments.get(2).is_some_and(|segment| is_git_route(segment))
                && segments
                    .get(3)
                    .is_some_and(|segment| is_git_object_id(segment))
        }
        "gitlab.com" => segments
            .windows(3)
            .find(|triple| triple[0] == "-" && is_git_route(triple[1]))
            .is_some_and(|triple| is_git_object_id(triple[2])),
        "bitbucket.org" => {
            segments.get(2) == Some(&"src")
                && segments
                    .get(3)
                    .is_some_and(|segment| is_git_object_id(segment))
        }
        _ => false,
    }
}

fn path_has_live_git_ref(path: &str, stop_at_immutable_pin: bool) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    for pair in segments.windows(2) {
        if stop_at_immutable_pin && is_git_object_id(pair[0]) {
            break;
        }
        let kind = pair[0].to_ascii_lowercase();
        if matches!(kind.as_str(), "blob" | "tree" | "raw" | "commits")
            && !is_git_object_id(pair[1])
        {
            return true;
        }
    }
    false
}

fn is_git_object_id(segment: &str) -> bool {
    (segment.len() == 40 || segment.len() == 64)
        && segment
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

#[cfg(test)]
mod tests {
    use super::{Ipv4Addr, ipv4_is_non_public, parse_credential_free_https_url};

    #[test]
    fn parsed_https_urls_reject_credentials() {
        for uri in [
            "https://reader@example.com/pinned-evidence",
            "https://:placeholder@example.com/pinned-evidence",
        ] {
            let err = parse_credential_free_https_url(uri).expect_err("credentials are forbidden");
            assert_eq!(err.code(), "KELD-COMPAT-007");
        }
        let uri = "https://example.com/pinned-evidence";
        let url = parse_credential_free_https_url(uri).expect("no credentials");
        assert_eq!(url.as_str(), uri);
    }

    #[test]
    fn ipv4_policy_classifies_special_use_and_public_unicast() {
        for (octets, expected_non_public) in [
            ([0, 1, 2, 3], true),         // This network
            ([192, 0, 0, 8], true),       // IETF protocol assignments
            ([192, 0, 0, 9], false),      // PCP anycast
            ([192, 0, 0, 10], false),     // TURN anycast
            ([192, 0, 0, 11], true),      // IETF protocol assignments
            ([192, 0, 2, 1], true),       // TEST-NET-1
            ([192, 88, 99, 1], true),     // Deprecated 6to4 relay anycast
            ([198, 18, 0, 1], true),      // Benchmarking lower /16
            ([198, 19, 255, 254], true),  // Benchmarking upper /16
            ([198, 51, 100, 1], true),    // TEST-NET-2
            ([203, 0, 113, 1], true),     // TEST-NET-3
            ([224, 1, 2, 3], true),       // Multicast
            ([239, 255, 0, 1], true),     // Multicast upper boundary
            ([240, 1, 2, 3], true),       // Reserved
            ([255, 255, 255, 255], true), // Limited broadcast
            ([8, 8, 8, 8], false),        // Public unicast (classification only)
        ] {
            let address = Ipv4Addr::from(octets);
            assert_eq!(
                ipv4_is_non_public(address),
                expected_non_public,
                "{address}"
            );
        }
    }
}
