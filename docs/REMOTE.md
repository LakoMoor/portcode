# Remote control

Drive a PortCode session from a browser or phone: watch the transcript, send prompts, answer permission requests, interrupt, switch agent or mode.

**How it works.** GitHub Pages hosts only a static page (`/portcode/remote/`). Your browser connects *directly* to `portcode serve` (or `/remote` inside the full-screen UI) over a WebSocket. There is no PortCode relay: nothing between you and your machine except the tunnel you choose.

```
browser (github.io page) ── wss ──▶ your tunnel ──▶ 127.0.0.1:7777  portcode serve
```

## Easiest: one command and a QR code
```sh
portcode remote tunnel-install   # once: downloads cloudflared (sha256-checked) into the engines folder
portcode serve --tunnel          # prints a link and a QR code; scan it with your phone
# or inside the full-screen UI: /remote   (opens the tunnel by itself when cloudflared is installed)
```
`serve --tunnel` also offers to download cloudflared the first time. It opens a free Cloudflare quick tunnel (no account), and prints `https://lakomoor.github.io/portcode/remote/#url=wss://….trycloudflare.com&key=pc1.…`. The part after `#` never leaves your browser, and the page connects with that key. The address changes on every run, so scan the new code each time. The link contains your key: do not share it. Anyone who learns the tunnel address still cannot connect without the key.

## Quick start
```sh
portcode serve                 # prints a key once, listens on 127.0.0.1:7777
# or, inside the full-screen UI:  /remote
```
Open `https://lakomoor.github.io/portcode/remote/`, enter the address and the key.
On the same machine use `ws://127.0.0.1:7777`. From elsewhere you need a tunnel, because GitHub Pages is `https` and browsers only allow `wss://` (plain `ws://` is allowed for `127.0.0.1` only):

| Tunnel | Command | Address to enter |
|---|---|---|
| Tailscale (private to your devices) | `tailscale serve --bg --https=443 http://127.0.0.1:7777` | `wss://<machine>.<tailnet>.ts.net` |
| Cloudflare quick tunnel | `cloudflared tunnel --url http://127.0.0.1:7777` | `wss://<random>.trycloudflare.com` |
| SSH (your own server) | `ssh -N -L 7777:127.0.0.1:7777 you@server` | `ws://127.0.0.1:7777` (Safari blocks this; use another browser) |

Tailscale is the best default: the address is reachable only from your own devices.

## Keys
```sh
portcode remote new [--scope view|full]   # shown once: pc1.<id>.<secret>
portcode remote list
portcode remote revoke <id>
```
- `full` can send prompts, answer permission asks, interrupt, switch agent/model and enable deep mode. `view` only watches.
- Keys live in the config directory (`remote-keys.json`, mode 0600).
- The secret never travels: the server sends a random nonce and the page answers with `HMAC-SHA256(secret, nonce)`. The page keeps the key in this browser's local storage; use "Forget saved" on shared computers.
- A pairing link `https://lakomoor.github.io/portcode/remote/#url=wss://…&key=pc1.…` fills the form; the page removes the fragment from the address bar. The fragment is never sent to GitHub.

## What protects you
- Binds `127.0.0.1` by default; `--bind 0.0.0.0` prints a warning (plain `ws://` is not encrypted).
- The server accepts browser connections only from `https://lakomoor.github.io` and `localhost`; add more with `PORTCODE_REMOTE_ORIGINS=https://my.example`.
- Wrong key: one-second delay, then disconnect. At most 4 clients, 1 MiB per message.
- Slash commands are not accepted remotely, so a remote client cannot quit your session; the page has explicit controls instead.
- **A `full` key is control of your agent**, which can run commands once permitted. Treat it like an SSH key. Deep mode asks for confirmation in the page, and permission rules still apply: anything that would ask you in the terminal asks the remote client.

## Protocol
JSON text frames. Server: `hello{nonce}` → (client `auth{id,mac}`) → `ready{scope,snapshot}`, then `text`, `tool_start`, `tool_call`, `tool_result`, `todos`, `usage`, `info`, `perm`, `perm_done`, `state`, `done`. Client (full): `send{text}`, `interrupt`, `perm{reply:once|always|reject}`, `agent{name}`, `model{ref}`, `deep{on}`, `clear`, `ping`.
