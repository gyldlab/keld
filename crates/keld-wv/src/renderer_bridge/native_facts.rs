//! Same-callback native URL metadata with closed platform provenance.
//! These facts never replace the live document/source/role checks at admission.

use crate::resource_source::NativeNavigationUrl;

#[derive(Debug)]
pub(crate) struct NativeMessageFacts {
    url: NativeNavigationUrl,
    provenance: NativeMessageProvenance,
}

#[derive(Debug)]
enum NativeMessageProvenance {
    #[cfg(target_os = "macos")]
    MacNativeOrigin {
        protocol: Option<NativeNavigationUrl>,
        host: Option<NativeNavigationUrl>,
        port: i64,
        serialized_origin: Option<NativeNavigationUrl>,
    },
    #[cfg(windows)]
    SourceQualifiedWindowsFile,
}

impl NativeMessageFacts {
    /// Preserve the actual WK tuple and its existing source-qualified observed
    /// serialization. Only the checked Mac callback capture constructs this.
    #[cfg(target_os = "macos")]
    pub(crate) fn from_macos_native(
        url: NativeNavigationUrl,
        protocol: Option<NativeNavigationUrl>,
        host: Option<NativeNavigationUrl>,
        port: i64,
        serialized_origin: Option<NativeNavigationUrl>,
    ) -> Self {
        Self {
            url,
            provenance: NativeMessageProvenance::MacNativeOrigin {
                protocol,
                host,
                port,
                serialized_origin,
            },
        }
    }

    /// Consume an actual native WebMessageReceived.Source carrier only after
    /// its owning callback has checked the retained sender/view. This performs
    /// the sole selected-file URL qualification and validates the same current
    /// source/lease; it creates no native commit, role or `SecurityOrigin` proof.
    ///
    /// The caller borrows its already-owned callback scratch reservation. No
    /// second reservation or account is created for this parser. Clone source
    /// and lease out of interior guards before calling: the shared Windows path
    /// grammar includes native NLS operations.
    #[cfg(windows)]
    pub(crate) fn from_windows_source_url(
        url: NativeNavigationUrl,
        source: &std::sync::Arc<crate::resource_source::AdmittedWebAppSource>,
        lease: &crate::renderer_contract::CheckedNativeDocumentLease,
        scratch: &crate::resource_intake::RetainedByteCredit,
    ) -> Result<Self, &'static str> {
        use crate::resource_intake::MAX_PENDING_METADATA_BYTES;
        let account = source.retained_credit_account();
        if !url.charged_to(&account) || !scratch.covers(&account, MAX_PENDING_METADATA_BYTES) {
            return Err("native message metadata lacks its existing same-source scratch credit");
        }
        if !lease.is_current() || !lease.belongs_to_source(source) {
            return Err("native message metadata lacks its current exact source lease");
        }
        let parsed = source
            .qualify_document_url(url.as_str())
            .map_err(|_| "native message URL is not the selected source document")?;
        drop(parsed); // Release actual parser scratch before the final lease observation.
        if !lease.is_current() || !lease.belongs_to_source(source) {
            return Err("native message source lease retired during URL qualification");
        }
        Ok(Self {
            url,
            provenance: NativeMessageProvenance::SourceQualifiedWindowsFile,
        })
    }

    pub(crate) fn url(&self) -> &str {
        self.url.as_str()
    }

    /// Mac carries the actual tuple's source-qualified serialization. Windows
    /// carries only the informational file-scheme serialization derived from
    /// the successfully qualified file URL, never a native `SecurityOrigin`.
    #[cfg_attr(
        windows,
        expect(
            clippy::unnecessary_wraps,
            reason = "the shared origin API preserves the absent native serialization possible on macOS"
        )
    )]
    pub(crate) fn origin(&self) -> Option<&str> {
        match &self.provenance {
            #[cfg(target_os = "macos")]
            NativeMessageProvenance::MacNativeOrigin {
                serialized_origin, ..
            } => serialized_origin.as_ref().map(NativeNavigationUrl::as_str),
            #[cfg(windows)]
            NativeMessageProvenance::SourceQualifiedWindowsFile => Some("file://"),
        }
    }

    /// Only WK supplies an actual same-message native origin tuple.
    /// `WebView2`'s qualified Source URL does not manufacture such a tuple.
    pub(crate) fn native_origin_tuple(&self) -> Option<(&str, &str, i64)> {
        match &self.provenance {
            #[cfg(target_os = "macos")]
            NativeMessageProvenance::MacNativeOrigin {
                protocol,
                host,
                port,
                ..
            } => Some((
                protocol.as_ref().map_or("", NativeNavigationUrl::as_str),
                host.as_ref().map_or("", NativeNavigationUrl::as_str),
                *port,
            )),
            #[cfg(windows)]
            NativeMessageProvenance::SourceQualifiedWindowsFile => None,
        }
    }
}

#[cfg(all(test, windows))]
#[path = "native_facts_tests.rs"]
mod tests;
