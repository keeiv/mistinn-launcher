<div align="center">

**EN** | [繁中](../CONTRIBUTING.md)

<img src="../src-tauri/icons/icon.png" alt="" width="64">

</div>

# Contributing

The Mistinn Launcher welcomes bug fixes, features, tests and documentation. Contributions come through a fork and a pull request; maintainers merge through pull requests too, and nothing is pushed straight to `main`.

## Scope

Small fixes and documentation can go straight to a PR. New dependencies, changes to where data is kept or to file formats (`launcher.json`, `accounts.dat`, instance folders), changes to the three ways to play, and anything touching sign-in start with an issue that explains the purpose and compatibility.

## Using AI tools

AI is welcome here. Contributions written, refactored or debugged with AI tools are treated the same as hand-written ones: no label needed, never a reason to reject.

There is one condition: **a pull request is merged only when it has no security problems.** Before you send it:

- You have read and understood every line, and can explain why it is written that way.
- The launcher never handles passwords: Microsoft sign-in uses only the device code flow, and tokens are saved only encrypted with DPAPI and never logged.
- Every download is checked by hash (SHA-1 for Mojang and Modrinth, the SHA-256 GitHub records for the embedded mod), and file names and paths never use unchecked input.
- `unsafe` is used only to call the Windows API, one operation per block, each with the reason it is sound (see "Memory" below).
- New dependencies exist, are maintained, and have no known serious advisories.

Comments are for whoever maintains the code next: say why it does what it does, and leave no conversation with a tool or editing history behind.

## Setup

You need Rust (the MSVC toolchain), Node.js 22, Python 3 and the Visual Studio Build Tools.

```powershell
git clone https://github.com/keeiv/mistinn-launcher.git
cd mistinn-launcher
npm ci
.\build.ps1 test
```

The first build downloads the jar to embed from the mod's latest release (`scripts/fetch-mod.ps1`) into `src-tauri/resources/mods`, which is not under version control.

## Checks

CI runs four groups of checks on every pull request, and all must pass before it merges. Run them locally before you commit:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml --check
npm run format:check
npm run lint
npm run check:comments
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets --locked -- -D warnings
.\build.ps1 test
```

When the formatting is off, `cargo fmt --manifest-path src-tauri/Cargo.toml` and `npm run format` fix it in place.

### Code style

- Rust is formatted by rustfmt (`rustfmt.toml`, at most 140 columns) and passes Clippy's `all` and `pedantic` groups, where any warning is a failure (`[lints]` in `Cargo.toml`).
- The window's script, styles and page (`src/`) are formatted by Prettier, and `main.js` also passes ESLint (`eslint.config.mjs`).

### Comment style

Checked by `scripts/check_comments.py` and Clippy's documentation lints:

- Every Rust file opens with a `//!` module comment; every function, struct, enum and constant at module level, and every `pub fn` in an `impl`, has a `///` doc comment. Test modules are exempt.
- A doc comment's first paragraph is short and reads as a sentence.
- `main.js` opens by saying what the file is, and every top-level function has a comment above it; `style.css` opens with one too.
- A line comment has a space after `//`; Rust uses no `/* */`; no TODO, FIXME, XXX or HACK is left in a comment.

### Memory

- `unsafe` is denied across the code (`unsafe_code = "deny"`) and allowed one function at a time with `#[allow(unsafe_code)]`, only to call the Windows API: today in `auth.rs` (DPAPI) and `hardware.rs` (DXGI). `unsafe` in any other file fails CI.
- Each `unsafe` block holds one unsafe operation, with a `// SAFETY:` comment above it saying why it is sound; memory Windows allocates for us is always freed, exactly once.
- Nothing is leaked on purpose (`mem::forget` is denied).
- Dependencies are checked against the RustSec database on every run (`cargo audit`): a known vulnerability fails the build, and unmaintained crates are listed for the reviewer.

## Commits and pull requests

Commit messages and PR titles use Conventional Commits prefixes with a short Traditional Chinese summary: `feat:`, `fix:`, `docs:`, `style:`, `refactor:`, `test:`, `ci:`, `chore:`.

A PR describes the problem, the new behaviour and how it was verified (checks run, screens looked at). Commit only files related to the change; never commit `node_modules/`, `target/`, `dist/`, `src-tauri/resources/mods/` or any keys.

## Releases

There is nothing to release by hand. The `Release` workflow checks every hour whether Mistinn: Exile has a new release, and if so builds and publishes a launcher with it built in, tagged `v<launcher>-mod.<mod>`. When the launcher's own code changes and needs a new version, raise the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `package.json` together; after the merge it is published within the hour, or run `Release` by hand from Actions.

## Reporting problems

Report on the [MoriTeahouse site](https://moriteahouse.shizune.life/account) or open an issue here, with the launcher version, the Windows version, the steps to reproduce, and what the game log shows.

## License

The launcher is licensed **AGPL-3.0-only**; the full text is in [LICENSE](../LICENSE). New contributions use the same license.
