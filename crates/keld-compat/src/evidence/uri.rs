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
        let (host, path) = parse_https_host_and_path(rest, trimmed)?;
        // Trailing FQDN dots are the same host (RFC 1034); compare once.
        let host = host.trim_end_matches('.').to_ascii_lowercase();
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
        if https_path_is_live_mutable(&host, &path) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has a live branch/tag path; \
                     a mutable URL is not an immutable pin"
                ),
            });
        }
        if !path_has_git_object_id(&path) {
            return Err(EvidenceError::NonNormativeEvidence {
                detail: format!(
                    "`{trimmed}` has no git object id in the path; \
                     a live-mutable URL is not an immutable pin"
                ),
            });
        }
        return Ok(trimmed.to_owned());
    }
    Err(EvidenceError::NonNormativeEvidence {
        detail: format!("`{trimmed}` is not https:// or sha256:"),
    })
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
    rest: &str,
    original: &str,
) -> Result<(String, String), EvidenceError> {
    let without_hash = rest.split_once('#').map_or(rest, |(p, _)| p);
    let without_query = without_hash
        .split_once('?')
        .map_or(without_hash, |(p, _)| p);
    if without_query.is_empty() || authority_has_userinfo(without_query) {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a public https location"),
        });
    }
    if without_query.starts_with('[') {
        let end = without_query
            .find(']')
            .ok_or_else(|| EvidenceError::NonNormativeEvidence {
                detail: format!("`{original}` is not a resolvable https URL with a path"),
            })?;
        let host = without_query[1..end].to_owned();
        let after = &without_query[end + 1..];
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
    if without_query.matches(':').count() > 1 {
        return Err(EvidenceError::NonNormativeEvidence {
            detail: format!("`{original}` is not a resolvable https URL with a path"),
        });
    }
    let slash = without_query.find('/');
    let hostport = slash.map_or(without_query, |i| &without_query[..i]);
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
    let path = slash.map_or(String::new(), |i| without_query[i..].to_owned());
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
    // Literal authority only: no DNS, and no abbreviated-IPv4 parser
    // (`127.1`, `010.0.0.1`, `1.1`) — those plus DNS-to-private (`nip.io`)
    // are T1 residuals (easy to mis-classify a public address).
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
    v4.is_loopback()
        || v4.is_unspecified()
        || v4.is_private()
        || v4.is_link_local()
        || ipv4_is_cgnat(v4)
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
    if path_has_live_git_ref(path) {
        return true;
    }
    let host = host.to_ascii_lowercase();
    let host = host.trim_end_matches('.');
    if host == "raw.githubusercontent.com" {
        let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        return segs
            .get(2)
            .is_some_and(|git_ref| !is_git_object_id(git_ref));
    }
    false
}

fn path_has_live_git_ref(path: &str) -> bool {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    segments.windows(2).any(|pair| {
        let kind = pair[0].to_ascii_lowercase();
        matches!(kind.as_str(), "blob" | "tree" | "raw") && !is_git_object_id(pair[1])
    })
}

fn is_git_object_id(segment: &str) -> bool {
    (segment.len() == 40 || segment.len() == 64)
        && segment
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}
