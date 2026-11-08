# WPF XAML support for Zed on Linux — design

Date: 2026-11-08
Status: draft, reviewed and grilled, awaiting approval

## Intent

The user develops a Windows WPF app (for `~/work/academia/pochodnia`, C# part not started yet) from Fedora Linux, in
Zed, without booting a Windows VM. Zed's existing C# extension handles `.cs` files but knows nothing about WPF XAML.
The user wants Visual Studio–style help while typing `.xaml`: completion of WPF controls and their properties, plus
the other things a XAML editor gives you.

A live XAML preview was discussed and dropped for now: Zed extensions cannot open custom panels or tabs (confirmed
in zed-industries/zed discussion #27163, still unshipped as of July 2026), and WPF does not run on Linux.

## Starting point (verified 2026-11-08)

- **Language server:** [Zurisen/wpf-xaml-language-server](https://github.com/Zurisen/wpf-xaml-language-server)
  (MIT, C#, .NET 10, alpha 0.1). The user forked it to
  [eRgo35/wpf-xaml-language-server](https://github.com/eRgo35/wpf-xaml-language-server) (default branch `master`).
  It already provides completion, hover, diagnostics and go-to-definition (XAML → C#), resource and binding
  resolution. See its `docs/architecture.md`.
- **Spike result on the user's machine:** the server builds and runs on Linux. Against `samples/SampleWpfApp` it
  returned 827 attribute completions after `<Button `, `Left/Center/Right/Stretch` for `HorizontalAlignment`,
  element completions after `<`, and hover docs for `Button`. Spike client: scratchpad `spike/lsp_probe.py`
  (throwaway; reuse its approach for verification, don't ship it).
- **Two problems found:**
  1. **NuGet cache path bug.** `FrameworkReferences.FindPack` only checks `$NUGET_PACKAGES` and
     `~/.nuget/packages`. The user's machine sets `DOTNET_CLI_HOME=~/.local/share/dotnet`, so the cache is
     `~/.local/share/dotnet/.nuget/packages` and the server found no WPF types (all completions empty) until
     `NUGET_PACKAGES` was set by hand.
  2. **Build fails by default.** `TreatWarningsAsErrors` + NuGet audit: transitive `MessagePack 2.5.302` has advisory
     GHSA-qhrr-8q5h-9q3h (NU1902).
- **Machine:** .NET SDK 10.0.113 at `/usr/lib64/dotnet` (Fedora package, `/usr/bin/dotnet` symlink), Rust/cargo
  installed, `gh` authenticated as the fork owner. WPF targeting packs 8.0.32, 10.0.12, 10.0.13 are in the NuGet
  cache.

## Goals

The user asked for "all improvements". That means all of the following, in this order:

### Part A — the fork (server)

1. **Update all dependencies** to current stable versions (Roslyn packages, Microsoft.Build.Locator, StreamJsonRpc,
   Nerdbank.Streams, xunit, GitHub Actions versions). The default `dotnet build` must succeed with no flags and no
   NU1902. Note the upstream author deliberately removed a `VersionOverride` (commit 894afbc): prefer fixing the
   advisory by upgrading the package that brings MessagePack in; pin it centrally only if no fixed upstream version
   exists.
2. **Fix the NuGet cache lookup on Linux.** Order: `$NUGET_PACKAGES`, then `$DOTNET_CLI_HOME/.nuget/packages`, then
   `~/.nuget/packages`. *(Builder's call: whether to also honor a `globalPackagesFolder` set in NuGet.Config. If
   that needs a new package such as NuGet.Configuration, ask first — the user's rule is no new dependencies without
   asking.)* Add a test.
3. **New LSP features**, each with tests in the existing test projects, following the existing layering (logic in
   `WpfXaml.Analysis` using offsets/`TextSpan`, LSP mapping only in `WpfXaml.LanguageServer`):
   - **Find references** (`textDocument/references`) for: resource keys (`x:Key` ↔ `StaticResource` /
     `DynamicResource` uses, across the solution's XAML files, using the existing resource scope rules), `x:Name`
     (↔ `ElementName=` uses in the same namescope), and XAML types/properties (other XAML uses in the solution).
   - **Rename** (`textDocument/rename` + `prepareRename`) for `x:Key` and `x:Name`, updating the same references.
     **`x:Name` and code-behind (user's decision):** the server's compilation never contains the WPF-generated
     `obj/*.g.cs` fields (a known gap in its `docs/architecture.md`), so Roslyn usually can't see C# uses of an
     `x:Name`. Rename and find-references therefore also cover the `x:Class` partial class's C# files (e.g.
     `MainWindow.xaml.cs`): identifier uses of the old name that don't bind to some other symbol (a local,
     parameter, member of another type, etc.) are treated as uses of the generated field. Use Roslyn syntax/semantic
     info for this, not plain text search. Own tests required, including a same-named local that must not be
     renamed. Rename of types/properties is out of scope (that's a C# rename).
   - **Formatting** (`textDocument/formatting`, and range formatting if cheap): re-indent elements, normalize
     whitespace between attributes, keep attribute order, keep comments and text content, never change meaning.
     Must be idempotent. Indent size/tabs from the LSP `FormattingOptions`. **No attribute wrapping** (user's
     decision): line breaks and attribute order stay as written; only indentation and inter-attribute spacing change.
   - **Outline** (`textDocument/documentSymbol`): the element tree, labelled with the element name plus `x:Name` or
     `x:Key` when present.
   - Advertise each new capability in `ServerCapabilities`; update the README feature list.
4. **Release pipeline:** a GitHub Actions workflow that, on a `v*` tag, builds a framework-dependent
   `dotnet publish` of the server and attaches one zip (e.g. `wpf-xaml-ls-<version>.zip`, containing
   `wpf-xaml-ls.dll` and dependencies) to a GitHub release. Framework-dependent is enough because MSBuildWorkspace
   needs a .NET SDK on the machine anyway. First release: **`v0.2.0`, a normal (not pre-) release**. Ask the user
   right before pushing that tag.
5. **Fork identity:** keep all names (`wpf-xaml-ls`, `WpfXaml.*`, NuGet id). Add a short "fork of
   Zurisen/wpf-xaml-language-server" note and the changes to README and CHANGELOG.
6. Existing CI keeps passing. Thanks to fix 2, the WPF-dependent tests that skip on `ubuntu-latest` may now run there
   if the runner restores the targeting pack; don't make CI depend on that.

### Part B — the Zed extension (this repo, `~/work/zed-dotnet`)

A Zed dev extension, written in Rust against `zed_extension_api` (current version):

- **Language "XAML"** for `*.xaml`, using the tree-sitter XML grammar Zed's own XML extension uses
  (`tree-sitter-grammars/tree-sitter-xml`, `xml` subdirectory; pin the same commit). Ship `config.toml` (block comment
  `<!-- -->`, brackets/autoclose for `<>` and quotes), `highlights.scm`, `brackets.scm`, `indents.scm`, and
  `outline.scm` if Zed's outline panel needs it (see open item below).
- **Language server "wpf-xaml-ls"** for XAML. How the extension finds the server, first hit wins:
  1. `lsp.wpf-xaml-ls.binary.path` from Zed settings (standard Zed mechanism);
  2. `wpf-xaml-ls` on the worktree's PATH (a `dotnet tool install -g` build);
  3. otherwise download the latest release zip from `eRgo35/wpf-xaml-language-server` into the extension's work
     dir and run `dotnet <dir>/wpf-xaml-ls.dll --stdio`. If `dotnet` is not on PATH, fail with a clear message.
     When a newer release exists, download it on the next server start and delete old versions.
- **Layout:** the extension sits at the repo root (`extension.toml`, `Cargo.toml`, `src/`, `languages/xaml/`), so
  "Install Dev Extension" points at `~/work/zed-dotnet` itself.
- Pass `initialization_options` from `lsp.wpf-xaml-ls.initialization_options` through to the server (`solution`,
  `loadProjects`, `frameworkVersion`).
- A README with install steps (Zed → Extensions → Install Dev Extension → this folder), the settings above, and the
  `EnableWindowsTargeting` note for WPF projects on Linux.

## Non-goals

- XAML preview / designer (revisit if Zed ships a panel or preview extension API).
- Publishing to Zed's extension registry.
- C# support — the user keeps the existing Zed C# extension.
- Code actions, hot reload, debugging.
- Opening PRs to the upstream Zurisen repo (the user may decide later).

## Anti-goals

- Formatting that rewrites attribute order or otherwise changes a file's meaning or diffs it noisily.
- Rename that silently leaves broken references behind.
- Requiring Windows or a VM for any part of the workflow.
- Machine-specific hacks (e.g. hardcoding `~/.local/share/dotnet` in the extension) instead of fixing the server.

## Constraints

- User's working rules (global CLAUDE.md): simplest solution that meets the ask; no speculative config flags; stop
  and ask on genuine ambiguity; no new dependencies without asking; delete scratch files when done.
- Git (user's decision): work directly on `master` in the fork and `main` here; commit and push freely. The one
  exception is the first release tag (ask first).
- Keep the server's existing architecture and style (see its `AGENTS.md`/`CLAUDE.md` if present, and
  `docs/architecture.md`).
- Building/running code cloned from GitHub may be blocked by the auto-mode safety check; the user can run such
  commands themselves with `!`.

## Decisions

**The user's:** reuse the existing server via their fork rather than writing one; no preview for now; update all
dependencies to latest stable, majors included, staying on .NET 10 (hold back and report any package whose bump
needs nontrivial code changes); add rename, references (incl. types/properties), formatting (no attribute
wrapping), outline; `x:Name` rename/references reach into code-behind; ship a Zed dev extension that auto-downloads
and auto-updates the server from the fork's releases; first release `v0.2.0` (normal release); clone at
`~/work/wpf-xaml-language-server`; work on the default branches; extension at repo root; keep the fork's names;
upstream PRs out of scope.

**Mine (flag for review):**
- Server distribution as a framework-dependent zip.
- Extension ids: extension `wpf-xaml`, language `XAML`, server `wpf-xaml-ls`.
- Rename covers `x:Key` and `x:Name` only.
- Extension license MIT, matching the server.

## Left to the builder

- Exact dependency versions; whether a fixed MessagePack comes via StreamJsonRpc or a central pin.
- Internal design of the new features inside `WpfXaml.Analysis`.
- **Outline in Zed:** check whether current Zed uses LSP `documentSymbol` for the outline panel or only tree-sitter
  `outline.scm`. Ship whatever makes the outline appear in Zed; the server's `documentSymbol` is still wanted for
  other editors.
- **Conflict with Zed's XML extension:** check whether it also claims `.xaml`. If so, document the
  `file_types` setting that sends `.xaml` to XAML.

## Verification

- Server: `dotnet build` and `dotnet test` pass with no flags, on the user's machine (Linux) with *no*
  `NUGET_PACKAGES` set. A probe like the spike's returns non-empty completions against `samples/SampleWpfApp`.
  New features have tests and a probe check.
- Extension: builds for wasm (`cargo build --target wasm32-wasip2` or whatever the current API requires); installs as
  a dev extension; in a test WPF project with `EnableWindowsTargeting`, the user sees completion, hover, diagnostics,
  go-to-definition, references, rename, formatting and outline working in Zed. The final in-Zed check is the
  user's, since it needs the GUI.
