use std::net::{IpAddr, UdpSocket};

/// This machine's LAN-facing IP, found by asking the OS which local
/// address it would use to route to the outside world.
/// `UdpSocket::connect` doesn't actually send anything for UDP — it
/// just resolves a route — so this works offline as long as there's a
/// default gateway configured.
pub fn local_lan_ip() -> Option<IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip())
}

/// True for any host that only makes sense to *this* machine and can
/// never be dialed from another device: loopback addresses
/// (`localhost`, `127.0.0.1`, `[::1]`) and the unspecified/wildcard
/// address (`0.0.0.0`, `[::]`). The latter is easy to end up with by
/// accident — a server bound to every interface reports its own
/// address as `0.0.0.0:PORT` (e.g. in a startup log, or
/// `listener.local_addr()`), and it's natural to assume that's the
/// address to actually open, even though it isn't a real destination
/// — connecting to it only "works" from the same machine, where the
/// OS treats it as loopback.
pub fn is_self_only_host(host: &str) -> bool {
    if let Ok(ip) = host.parse::<IpAddr>() {
        return ip.is_loopback() || ip.is_unspecified();
    }
    if let Some(bracketed) = host.strip_prefix('[').and_then(|rest| rest.split(']').next())
        && let Ok(ip) = bracketed.parse::<IpAddr>()
    {
        return ip.is_loopback() || ip.is_unspecified();
    }
    let host_only = host.rsplit_once(':').map(|(h, _)| h).unwrap_or(host);
    host_only == "localhost" || host_only.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback() || ip.is_unspecified())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_hosts_are_self_only() {
        assert!(is_self_only_host("localhost"));
        assert!(is_self_only_host("localhost:3000"));
        assert!(is_self_only_host("127.0.0.1"));
        assert!(is_self_only_host("127.0.0.1:3000"));
        assert!(is_self_only_host("::1"));
        assert!(is_self_only_host("[::1]"));
        assert!(is_self_only_host("[::1]:3000"));
    }

    #[test]
    fn unspecified_hosts_are_self_only() {
        assert!(is_self_only_host("0.0.0.0"));
        assert!(is_self_only_host("0.0.0.0:3000"));
        assert!(is_self_only_host("::"));
        assert!(is_self_only_host("[::]"));
        assert!(is_self_only_host("[::]:3000"));
    }

    #[test]
    fn lan_hosts_are_not_self_only() {
        assert!(!is_self_only_host("192.168.1.42:3000"));
        assert!(!is_self_only_host("10.0.0.5:3000"));
        assert!(!is_self_only_host("shop.example.com"));
    }
}
