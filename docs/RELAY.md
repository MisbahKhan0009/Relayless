# Fast transfers when a direct path is blocked

Relayless always tries to connect the two devices directly. Between two home
connections in the same country that usually works. Across countries, on
mobile data, or behind an ISP that shares one public address among many
customers (carrier-grade NAT, common in Bangladesh), routers often refuse the
direct path. Relayless then sends the data through a **relay**.

The relay only forwards end-to-end encrypted packets. It can't read them and
doesn't store them. But the free public relays run by the iroh project are
**rate-limited** and meant for testing, so a relayed transfer is often stuck
around 1 Mbit/s.

## 1. Check whether your transfer is relayed

- While downloading, a transfer going through a relay shows an amber **Relayed**
  label next to its speed. Files you send show the same label.
- Afterwards, the privacy receipt in **History** says *Direct, device to device*
  or how many bytes went through a relay.

If it says **Direct** and the speed is still low, the limit is the internet
connection itself (usually the sender's upload speed). A relay won't help.

## 2. Try to get a direct path (free)

- **Turn on UPnP or NAT-PMP** in both routers' settings. On a FRITZ!Box: *Internet
  → Permit Access → Port Sharing → allow independent port sharing for this
  device*. Relayless then asks the router to open a port by itself.
- **Use IPv6** if your ISP offers it. Two devices with IPv6 usually connect
  directly, with no NAT in the way.
- Prefer home broadband over mobile data on at least one side.

## 3. Run your own relay (about €4 a month, full speed)

Your own relay has no rate limit. Put it close to one of the two devices: for
Bangladesh to Germany, a server in Germany adds almost no detour.

**You need:** a small Linux server with a public IP. For example, Hetzner Cloud
CX22 in Falkenstein or Nuremberg (20 TB traffic included), or an Oracle Cloud
*Always Free* instance. You also need a host name pointing to it: your own
domain (`relay.example.org`), or a free name like `203-0-113-7.sslip.io` that
resolves to the IP address in it.

### Install

On the server (Ubuntu or Debian):

```sh
sudo apt update && sudo apt install -y build-essential curl
curl https://sh.rustup.rs -sSf | sh -s -- -y
source ~/.cargo/env
# Same iroh version as the app
cargo install iroh-relay@1.4.1 --features server --locked
sudo install ~/.cargo/bin/iroh-relay /usr/local/bin/
```

### Configure

Create `/etc/iroh-relay/config.toml`:

```toml
http_bind_addr = "[::]:80"

[tls]
https_bind_addr = "[::]:443"
hostname = "relay.example.org"        # your host name
cert_mode = "LetsEncrypt"
cert_dir = "/var/lib/iroh-relay/certs"
contact = "you@example.org"           # for the certificate

# Optional: only your devices may use this relay.
# Device IDs are in Relayless under Settings → About.
# [access]
# allowlist = ["<device id of PC>", "<device id of phone>"]
```

Open TCP ports 80 and 443 in the server's firewall. If you also set
`enable_quic_addr_discovery = true` at the top of the file, open UDP 7842 too;
it helps the two devices find a direct path.

### Run it as a service

```sh
sudo mkdir -p /var/lib/iroh-relay/certs
sudo tee /etc/systemd/system/iroh-relay.service >/dev/null <<'EOF'
[Unit]
Description=iroh relay for Relayless
After=network-online.target

[Service]
ExecStart=/usr/local/bin/iroh-relay --config-path /etc/iroh-relay/config.toml
Restart=always

[Install]
WantedBy=multi-user.target
EOF
sudo systemctl enable --now iroh-relay
curl --fail https://relay.example.org/healthz    # should answer OK
```

### Use it

On **both** devices, open **Settings → Connection → Relay server** and enter
the host name (for example `relay.example.org`). Then quit and reopen
Relayless. Settings → About → Network should now show your relay.

From then on, relayed transfers run as fast as the slower of the two internet
connections and your server's bandwidth allow. Everything stays end-to-end
encrypted: your relay sees the same thing the public ones do, which is
encrypted packets and device IDs, never file names or contents.
