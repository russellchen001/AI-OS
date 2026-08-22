use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadProtocol {
    Http,
    Ftp,
    Magnet,
    Torrent,
    Thunder,
    Ed2k,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadExecutionRoute {
    Direct,
    Web,
    Search,
    Aria2,
    P2p,
    Unsupported,
}

pub fn detect_protocol(source: &str) -> DownloadProtocol {
    let value = source.trim().to_lowercase();

    if value.starts_with("thunder://") {
        return DownloadProtocol::Thunder;
    }

    if value.starts_with("ed2k://") {
        return DownloadProtocol::Ed2k;
    }

    if value.starts_with("magnet:") {
        return DownloadProtocol::Magnet;
    }

    if value.ends_with(".torrent") {
        return DownloadProtocol::Torrent;
    }

    if value.starts_with("ftp://") {
        return DownloadProtocol::Ftp;
    }

    if value.starts_with("http://") || value.starts_with("https://") {
        return DownloadProtocol::Http;
    }

    DownloadProtocol::Unknown
}

pub fn resolve_openclaw_download(source: &str) -> (DownloadExecutionRoute, String) {
    if detect_protocol(source) == DownloadProtocol::Thunder {
        return decode_thunder_source(source)
            .map(|decoded| {
                let (route, _) = resolve_openclaw_download(&decoded);
                (route, decoded)
            })
            .unwrap_or_else(|| {
                (
                    DownloadExecutionRoute::Unsupported,
                    source.trim().to_string(),
                )
            });
    }

    let normalized = source.trim().to_ascii_lowercase();

    let route = match detect_protocol(source) {
        DownloadProtocol::Http => {
            let path = normalized.split(['?', '#']).next().unwrap_or(&normalized);
            if [
                ".7z", ".dmg", ".exe", ".gz", ".iso", ".msi", ".pdf", ".pkg", ".rar", ".tar",
                ".tgz", ".zip", ".bin",
            ]
            .iter()
            .any(|extension| path.ends_with(extension))
            {
                DownloadExecutionRoute::Direct
            } else {
                DownloadExecutionRoute::Web
            }
        }
        DownloadProtocol::Magnet | DownloadProtocol::Torrent | DownloadProtocol::Ed2k => {
            DownloadExecutionRoute::P2p
        }
        DownloadProtocol::Ftp => DownloadExecutionRoute::Aria2,
        DownloadProtocol::Thunder => DownloadExecutionRoute::Unsupported,
        DownloadProtocol::Unknown if !source.trim().is_empty() => DownloadExecutionRoute::Search,
        DownloadProtocol::Unknown => DownloadExecutionRoute::Unsupported,
    };
    (route, source.trim().to_string())
}

pub fn select_openclaw_route(source: &str) -> DownloadExecutionRoute {
    resolve_openclaw_download(source).0
}

fn decode_thunder_source(source: &str) -> Option<String> {
    let encoded = source.trim().strip_prefix("thunder://")?;
    let decoded = String::from_utf8(STANDARD.decode(encoded).ok()?).ok()?;
    decoded
        .strip_prefix("AA")?
        .strip_suffix("ZZ")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn detects_protocols() {
        assert_eq!(detect_protocol("magnet:?xt=test"), DownloadProtocol::Magnet);

        assert_eq!(
            detect_protocol("https://example.com/file.zip"),
            DownloadProtocol::Http
        );

        assert_eq!(
            detect_protocol("ftp://example.com/file"),
            DownloadProtocol::Ftp
        );
    }

    #[test]
    fn automatic_routes_never_require_thunder_ui_confirmation() {
        assert_eq!(
            select_openclaw_route("magnet:?xt=urn:btih:test"),
            DownloadExecutionRoute::P2p
        );
        assert_eq!(
            select_openclaw_route("ed2k://example"),
            DownloadExecutionRoute::P2p
        );
    }

    #[test]
    fn thunder_wrapper_is_decoded_before_automatic_routing() {
        let source = "thunder://QUFodHRwczovL2V4YW1wbGUuY29tL2ZpbGUuemlwWlo=";
        assert_eq!(
            resolve_openclaw_download(source),
            (
                DownloadExecutionRoute::Direct,
                "https://example.com/file.zip".to_string()
            )
        );
    }

    #[test]
    fn openclaw_routes_web_cloud_and_unsupported_sources() {
        assert_eq!(
            select_openclaw_route("https://example.com/file.zip"),
            DownloadExecutionRoute::Direct
        );
        assert_eq!(
            select_openclaw_route("https://example.com/download"),
            DownloadExecutionRoute::Web
        );
        assert_eq!(
            select_openclaw_route("https://cloud.example/share/example"),
            DownloadExecutionRoute::Web
        );
        assert_eq!(
            select_openclaw_route("Ubuntu 24.04 desktop ISO"),
            DownloadExecutionRoute::Search
        );
    }
}
