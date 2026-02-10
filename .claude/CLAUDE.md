# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is kemote

kemote (kitty emotes picker) is a Wayland/Linux desktop emote picker that searches 7TV emotes via their GraphQL API, displays results in a grid, and copies selected emote images to the clipboard. Built with Rust using GPUI (Zed's UI framework).

## Build & Run

Uses Nix flakes. Enter dev shell first: `nix develop`

- **Run**: `cargo run` (inside dev shell)
- **Build release**: `nix build` (outputs `result/bin/kemote`, includes patchelf for Wayland/Vulkan/libxkbcommon)

No tests or linting are configured. Rust edition is 2024.

## Architecture

Single-binary GUI app with three source files:

- **`src/main.rs`** — Entry point and all UI. `InputExample` is the root view composing search input + emote grid. `TextInput` handles text editing via GPUI's `EntityInputHandler` and triggers 7TV queries with 200ms debounce (AtomicBool cancels stale queries). `DisplayedEmote` handles click → WebP decode → clipboard copy via `wl-clipboard-rs`. `RecentEmotes` is an LRU deque (capacity 15) persisted to `recent.json`.

- **`src/seventv.rs`** — 7TV GraphQL API client. `query_7tv()` posts an embedded GQL query to `api.7tv.app/v4/gql`, extracts 4x scale WebP URLs from the nested response.

- **`src/cache.rs`** — Custom `ImageCache` implementation that intercepts URI loads: downloads WebPs to disk first, then loads from the local file. Implements GPUI's `ImageCache` trait.

## Key Details

- **Cache location**: `~/.cache/kemote/` (or `~/.cache/dev-kemote/` in debug builds). Subdirs: `queries/` (search results as JSON, keyed by SHA-256 of query), `webm/` (downloaded WebPs, keyed by SHA-256 of URL).
- **VERSION constant** in `main.rs:31` must stay in sync with `Cargo.toml` version.
- GPUI is pulled from the Zed repo via git dependencies (along with `gpui_tokio`, `util`, `reqwest_client`, `http_client`).
