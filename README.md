<p align="center">
  <img src="assets/gray-logo.svg" alt="gray" width="96">
</p>
<h1 align="center">gray-cloud</h1>
<p align="center">Your Gray Cloud machine on gray.alignment.id — pair, provision, and chat from the terminal.</p>
<p align="center">
  <a href="https://github.com/vstaln/gray-cloud/blob/main/LICENSE"><img alt="MIT License" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="gray plugin" src="https://img.shields.io/badge/gray-plugin-7aa2f7.svg">
  <img alt="rust" src="https://img.shields.io/badge/built%20with-rust-orange.svg">
</p>

Connect this terminal to your Gray Cloud machine on
[gray.alignment.id](https://gray.alignment.id) — a plugin for
[gray](https://github.com/vstaln/gray). Pairing exchanges a one-time
enrollment code from the account page for a registry token; the first
`status` or chat call provisions the machine, and any other words are
sent to it as a chat message.

## Install

```sh
gray plugin install cloud
```

## Usage

```sh
gray cloud connect [code]   # pair this terminal with a one-time code (prompts if omitted)
gray cloud status           # provision if needed and print the machine's state
gray cloud logout           # revoke the stored token and forget the pairing
gray cloud <message>        # chat with your machine (first call provisions it)
```

Mint a login code at <https://gray.alignment.id/account> — sign in with
GitHub, Google, or Discord and choose "Generate CLI login code" (it
expires in 5 minutes). `connect` with no argument prints those steps and
prompts for the code.

Bare `gray cloud` is `status`; an empty message falls back to status
too. A suspended seat is reported as such.

## The token

Exchanged tokens are written to `~/.gray/registry-token.json`, mode
`0600`, via an atomic rename — the same file `gray account` uses, so one
pairing serves both plugins. `logout` asks the registry to revoke the
token before deleting the file locally.

`GRAY_REGISTRY_URL` overrides the registry base URL (default
`https://gray.alignment.id/api`) for pointing at a local backend; plain
http is only accepted for loopback hosts.

---

Part of the [gray](https://github.com/vstaln/gray) plugin ecosystem —
the open-source AI agent harness. <https://gray.alignment.id>
