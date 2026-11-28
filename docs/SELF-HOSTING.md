# Running your own sync server

`housewright-server` is one program with no other services: it keeps a
SQLite file and a folder of encrypted files. It stores and relays only data
it cannot read (see [SECURITY.md](SECURITY.md)). There are no accounts:
devices are identified by their keys.

You need a machine that your phones can reach (a small VPS, a home server
with a domain name, a Raspberry Pi) and, for anything beyond your own
computer, HTTPS. The app refuses plain `http://` addresses except
`localhost` (D-96).

## 1. Build or get the binary

```sh
git clone https://github.com/holdTheDoorHoid/housewright
cd housewright
cargo build --release --locked -p housewright-server
sudo install target/release/housewright-server /usr/local/bin/
```

## 2. Run it with systemd, behind Caddy (recommended)

Caddy gets and renews a TLS certificate by itself.

```sh
sudo useradd --system --home /var/lib/housewright housewright
sudo cp crates/server/deploy/housewright-server.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now housewright-server
```

`/etc/caddy/Caddyfile`:

```
housewright.example.org {
    reverse_proxy 127.0.0.1:8787
}
```

```sh
sudo systemctl reload caddy
curl https://housewright.example.org/v1/health
```

In the app: Settings, Sync and devices, "Your own server", type
`https://housewright.example.org` and the registration token the server
printed when it started (`journalctl -u housewright-server`, or the file
`registration-token` in the data folder).

With nginx instead, proxy to `127.0.0.1:8787`, pass
`X-Forwarded-For`, and set `client_max_body_size 30m;` (files can be up to
26 MB). If your proxy serves the app under a path that it does not strip
(`https://example.org/housewright/`), start the server with
`--base-path /housewright`.

## 3. Or serve HTTPS directly

```sh
housewright-server --listen 0.0.0.0:8443 --data-dir /var/lib/housewright \
  --tls-cert /etc/housewright/fullchain.pem --tls-key /etc/housewright/privkey.pem
```

## 4. Or with Docker

```sh
docker build -f crates/server/deploy/Dockerfile -t housewright-server .
docker run -d --name housewright -p 127.0.0.1:8787:8787 -v hw-data:/data housewright-server
```

and put Caddy in front as above.

## Settings

Every flag also reads an environment variable.

| Flag | Variable | Default | Meaning |
|---|---|---|---|
| `--listen` | `HW_LISTEN` | `127.0.0.1:8787` | Address and port. |
| `--data-dir` | `HW_DATA_DIR` | `./housewright-data` | Database and encrypted files. |
| `--tls-cert`, `--tls-key` | `HW_TLS_CERT`, `HW_TLS_KEY` | none | Serve HTTPS with these PEM files. |
| `--behind-proxy` | `HW_BEHIND_PROXY` | off | Plain HTTP for a local proxy; trusts its `X-Forwarded-For` for rate limits. |
| `--base-path` | `HW_BASE_PATH` | empty | Path prefix the proxy does not strip. |
| `--registration-token` | `HW_REGISTRATION_TOKEN` | made on first start | Adding a household needs this token (typed once in the app). Without the flag the server makes one, keeps it in `registration-token` in the data folder, and prints it at start (D-105). |
| `--open-registration` | `HW_OPEN_REGISTRATION` | off | Anyone who can reach the server may add a household. Only on a private network. |
| `--max-households` | `HW_MAX_HOUSEHOLDS` | 20 | A cap on households. It does not limit storage on its own. |
| `--household-quota-mb` | `HW_HOUSEHOLD_QUOTA_MB` | 2048 | The most one household may store (changes and files). With the household cap, this bounds disk use. |

## Backups of the server

Back up the data folder (stop the server, or copy `housewright-server.db`
with `sqlite3 .backup`, plus `blobs/`). A server backup is encrypted data
only: on its own it is useless to anyone without a device or a recovery
key, and it is not a substitute for the app's own full backup. Restoring an
old server backup can lose changes made after it (see SECURITY.md, known
weaknesses); prefer keeping the server's disk healthy.

## What the server logs

Its start-up line, warnings and storage errors (kind only). It never logs
request bodies, paths or household ids.
