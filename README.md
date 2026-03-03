# Tetrust (Rust + WASM)

A compact, canvas-based Tetris implementation in Rust, bundled with Trunk, deployable to GitHub Pages.

Gameplay features:
- 10×20 matrix with 2 hidden spawn rows
- 7‑bag randomizer
- SRS rotations (JLSTZ + I kick tables)
- Hold + Next queue
- Ghost piece
- Lock delay
- Mobile-friendly on-screen controls

## Local dev

Prereqs:
- Rust (stable)
- `wasm32-unknown-unknown` target
- Trunk

Install:
```bash
rustup target add wasm32-unknown-unknown
cargo install --locked trunk
```

Run:
```bash
trunk serve --open
```

Release build:
```bash
trunk build --release
```

## GitHub Pages

This repo includes a workflow at `.github/workflows/pages.yml` that:
- builds with Trunk
- uploads `dist/` as a Pages artifact
- deploys via `actions/deploy-pages`

In GitHub:
Settings → Pages → “Build and deployment” → Source: **GitHub Actions**

Push to `main` and it deploys.

## Controls

Keyboard:
- ←/→ Move
- ↑ Rotate CW
- Z Rotate CCW
- ↓ Soft drop
- Space Hard drop
- C / Shift Hold
- P Pause
- R Restart

Mobile: use the on-screen controls bar.

## About

By GPT-5.2 Pro and Codex.