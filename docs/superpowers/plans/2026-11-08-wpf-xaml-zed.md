# WPF XAML in Zed Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the user's fork of wpf-xaml-ls build cleanly and work on Linux, add references/rename/formatting/outline, publish it as a GitHub release, and wrap it in a Zed dev extension that auto-downloads it.

**Architecture:** Part A changes the C# server in `~/work/wpf-xaml-language-server`, following its layering: logic in `WpfXaml.Analysis` (offsets/`TextSpan`, `XamlLocation`), LSP mapping only in `WpfXaml.LanguageServer`. Part B is a Rust→WASM Zed extension at the root of `~/work/zed-dotnet` that registers a XAML language (tree-sitter XML grammar) and launches the server.

**Tech Stack:** .NET 10, Roslyn, StreamJsonRpc, xunit v3; Rust, `zed_extension_api` 0.7.0; GitHub Actions + `gh`.

**Spec:** `docs/superpowers/specs/2026-11-08-wpf-xaml-zed-design.md` (this repo). Read it first.

## Global Constraints

- Simplest solution that meets the spec. No speculative options or flags. Stop and ask on real ambiguity.
- **No new dependencies (NuGet packages, crates, GitHub Actions) without asking the user.** Version bumps of existing ones are fine.
- Git: commit directly on `master` (fork) and `main` (this repo); push freely. **Ask the user before pushing the `v0.2.0` tag.**
- Stay on .NET 10 (`net10.0`, `global.json` SDK 10.0.100 rollForward latestFeature).
- Keep the fork's names: `wpf-xaml-ls`, `WpfXaml.*`, NuGet id `WpfXaml.LanguageServer`.
- Follow the server's `CONTRIBUTING.md`: report only what is certain; every check gets tests for both sides.
- Building or running code from the fork may be blocked by the auto-mode safety check. If so, give the user the exact command to run with `!`.
- Linux machine facts: dotnet at `/usr/lib64/dotnet`; `DOTNET_CLI_HOME=~/.local/share/dotnet`; NuGet cache `~/.local/share/dotnet/.nuget/packages`.
- Delete scratch files (scratchpad `spike/`, `src/` clone) at the end.

## Review Focus

1. **CRLF files.** WPF projects created on Windows use `\r\n`. Formatting must keep line endings and stay idempotent. Test in Task 7.
2. **Half-typed XAML.** Formatting a document with syntax errors returns no edits. Rename and references on broken markup don't throw. Tests in Tasks 5 and 7.
3. **Template namescopes.** An `x:Name` inside a `ControlTemplate`/`DataTemplate` is a different name from the same name on the window. References and rename must not mix them. Test in Task 4.
4. **Invalid new names.** Rename to `""`, `"a b"`, `"a\"b"`, or (for `x:Name`) a C# keyword like `class` is rejected with an error message, not applied. Test in Task 5.
5. **Offline Zed start.** If GitHub can't be reached (offline, rate-limited), the extension uses the newest already-downloaded server instead of failing. Manual check in Task 10.

---

## Part A — the fork (`~/work/wpf-xaml-language-server`, branch `master`)

### Task 1: Update dependencies; clean default build

**Files:**
- Modify: `Directory.Packages.props`, `.github/workflows/ci.yml` (action versions), `global.json` only if needed

- [ ] **Step 1:** Run `dotnet list WpfXamlLs.slnx package --outdated --include-transitive` and record the current → latest stable versions.
- [ ] **Step 2:** Bump every `PackageVersion` to latest stable. For NU1902 (MessagePack 2.5.302, GHSA-qhrr-8q5h-9q3h), first check whether the new StreamJsonRpc brings a fixed MessagePack. Only if not, add a central `PackageVersion` pin for `MessagePack` plus a comment saying why. Don't use `VersionOverride` (upstream commit 894afbc removed one on purpose).
- [ ] **Step 3:** Run `dotnet build WpfXamlLs.slnx -c Release`. Expected: `0 Error(s)` with no extra flags. If a major bump needs nontrivial code changes, revert that package, note it for the user, and continue.
- [ ] **Step 4:** Run `dotnet test --solution WpfXamlLs.slnx -c Release`. Expected: same pass/skip counts as before the bump (record the "before" numbers in Step 1).
- [ ] **Step 5:** Bump the action versions in `ci.yml` if newer majors exist. Commit with the message `Update dependencies; fix MessagePack advisory (NU1902)`.

### Task 2: Find the NuGet cache via DOTNET_CLI_HOME

**Files:**
- Modify: `src/WpfXaml.Analysis/Framework/FrameworkReferences.cs` (`FindPack`, around line 105)
- Test: `tests/WpfXaml.Analysis.Tests/FrameworkReferencesTests.cs` (create)

**Interfaces:**
- Produces: `internal static string GetNuGetPackagesRoot(Func<string, string?> getEnvironmentVariable, string userProfile)` (add `InternalsVisibleTo` for the test project if it isn't there yet)

- [ ] **Step 1: Write failing tests**

```csharp
[Fact] public void NuGetRoot_PrefersNuGetPackagesVariable() =>
    Assert.Equal("/x/pkgs", FrameworkReferences.GetNuGetPackagesRoot(
        v => v switch { "NUGET_PACKAGES" => "/x/pkgs", "DOTNET_CLI_HOME" => "/cli", _ => null }, "/home/u"));
[Fact] public void NuGetRoot_UsesDotnetCliHome() =>
    Assert.Equal(Path.Combine("/cli", ".nuget", "packages"), FrameworkReferences.GetNuGetPackagesRoot(
        v => v == "DOTNET_CLI_HOME" ? "/cli" : null, "/home/u"));
[Fact] public void NuGetRoot_FallsBackToUserProfile() =>
    Assert.Equal(Path.Combine("/home/u", ".nuget", "packages"), FrameworkReferences.GetNuGetPackagesRoot(_ => null, "/home/u"));
```

- [ ] **Step 2:** Run `dotnet test --solution WpfXamlLs.slnx --filter-class "*FrameworkReferencesTests"`. Expected: FAIL (method missing).
- [ ] **Step 3:** Implement it, and have `FindPack` use it with `Environment.GetEnvironmentVariable` and the user profile. Empty strings count as unset.
- [ ] **Step 4:** Run the full test suite **with `NUGET_PACKAGES` unset**. Expected: the new tests pass, and tests that used to skip ("No WindowsDesktop targeting pack found") now run on this Linux machine. If some of them fail only on Linux, fix the cause when it's a Linux path or line-ending issue in the code. Otherwise report it and mark it `Skip` with the reason.
- [ ] **Step 5:** Commit with the message `Find the NuGet cache through DOTNET_CLI_HOME`.

### Task 3: Outline (`textDocument/documentSymbol`)

**Files:**
- Create: `src/WpfXaml.Analysis/Features/DocumentSymbolService.cs`
- Modify: `src/WpfXaml.LanguageServer/XamlLanguageServer.cs` (capability + handler), `src/WpfXaml.LanguageServer/Protocol/LspTypes.cs` (DocumentSymbol types), `LspConverters.cs`
- Test: `tests/WpfXaml.Analysis.Tests/DocumentSymbolTests.cs`, extend `tests/WpfXaml.LanguageServer.Tests/LanguageServerTests.cs`

**Interfaces:**
- Produces: `public sealed record XamlSymbol(string Name, bool IsPropertyElement, TextSpan Span, TextSpan NameSpan, IReadOnlyList<XamlSymbol> Children);` and `public static IReadOnlyList<XamlSymbol> DocumentSymbolService.GetSymbols(XamlDocument document)`. This is syntax only and needs no type system.

- [ ] **Step 1: Write failing tests**
  - `Symbols_NestedTree`: for `<Window><Grid><Button x:Name="ok"/></Grid></Window>`, the result is one root `Window`, whose child is `Grid`, whose child has Name `Button ok`.
  - `Symbols_UsesKeyOrNameAttribute`: `<SolidColorBrush x:Key="Accent"/>` gives `SolidColorBrush Accent`, and `<Button Name="b"/>` gives `Button b`. When both are present, `x:Name` wins over `Name`, which wins over `x:Key`.
  - `Symbols_PropertyElementsFlagged`: `<Grid.RowDefinitions>` has `IsPropertyElement == true`.
  - `Symbols_EmptyNameElementSkipped`: a bare `<` produces no symbol and doesn't throw.
  - Server test `DocumentSymbol_ReturnsHierarchy`: the capability `documentSymbolProvider == true`, and the response root name is `Window`.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement the service. In the server, map to LSP `DocumentSymbol[]`, with `SymbolKind.Class` (5) for objects and `SymbolKind.Property` (7) for property elements. `range` = element span, `selectionRange` = name span.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with the message `Add document symbols (outline)`.

### Task 4: Find references (XAML side)

**Files:**
- Create: `src/WpfXaml.Analysis/Features/ReferenceService.cs`
- Modify: `XamlLanguageServer.cs` (capability `referencesProvider`, handler `textDocument/references`), `LspTypes.cs` (`ReferenceParams` with `context.includeDeclaration`)
- Test: `tests/WpfXaml.Analysis.Tests/ReferenceTests.cs`, a server test

**Interfaces:**
- Consumes: `XamlTargetResolver.Resolve`, `XamlSemanticModel.GetResourceScope`, `ResourceSyntax.GetStringKey`, `ProjectSystem.GetAllXamlFiles()`, `XamlLocation`
- Produces:
  - `public static IReadOnlyList<XamlLocation> ReferenceService.FindReferences(XamlSemanticModel model, int offset, IReadOnlyList<XamlSemanticModel> otherDocuments, bool includeDeclaration)`. `FilePath` is null for `model` itself, and `other.Resources?.FilePath` for other documents.
  - `public static XamlNameTarget? ReferenceService.FindNameTarget(XamlSemanticModel model, int offset)`, used by Tasks 5 and 6. `public sealed record XamlNameTarget(XamlNameKind Kind, string Name, XamlElement DeclaringElement)`, with `enum XamlNameKind { ResourceKey, ElementName }`.

What counts as a reference:
- **Resource key** (caret on an `x:Key` value or a `{StaticResource K}`/`{DynamicResource K}` key): the definition, plus every Static/Dynamic use in this and other documents whose `GetResourceScope(useElement).Find(K)` contains that same definition (same file path and `KeySpan`).
- **x:Name** (caret on an `x:Name`/`Name` value or an `ElementName=` value): the declaration plus `ElementName=` uses in the **same namescope**. A namescope is the document root, or the nearest `ControlTemplate`/`DataTemplate`/`ItemsPanelTemplate`/`HierarchicalDataTemplate` ancestor. Same file only. Code-behind is added in Task 6.
- **Type or member** (`XamlTargetKind.Type`/`Member`/`MarkupExtension`): every element name or attribute name in this and other documents that resolves to the same symbol (`SymbolEqualityComparer.Default`).
- Server: the other documents are `GetAllXamlFiles()` minus the current one, with open buffers preferred over disk, each with a semantic model from `_projects.GetTypeSystemAsync(path)`.

- [ ] **Step 1: Write failing tests** (use `TestWorkspace`, build the other documents with `TestWorkspace.Model`)
  - `References_ResourceKeyAcrossUses`: a `Window.Resources` key `Accent` used twice through StaticResource gives 3 locations with `includeDeclaration`, and 2 without.
  - `References_ResourceKeyShadowedNotIncluded`: an inner `Grid.Resources` redefines `Accent`. Uses inside the Grid don't count as references of the outer key.
  - `References_ElementName`: `x:Name="ok"` plus `{Binding IsEnabled, ElementName=ok}` gives 2.
  - `References_TemplateNamescopeSeparate` (Review Focus 3): `ok` on the window and `ok` inside a `ControlTemplate` are not each other's references.
  - `References_TypeAcrossDocuments`: `local:FancyButton` used in two documents gives both locations, the second with that document's `FilePath`.
  - `References_BrokenMarkupNoThrow` (Review Focus 2): a caret after a bare `<` returns an empty list.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement the service and the server handler.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with the message `Add find references for resource keys, names, types and members`.

### Task 5: Rename (`x:Key`, `x:Name`)

**Files:**
- Create: `src/WpfXaml.Analysis/Features/RenameService.cs`
- Modify: `XamlLanguageServer.cs` (`renameProvider: { prepareProvider: true }`, handlers `textDocument/prepareRename`, `textDocument/rename`), `LspTypes.cs` (`RenameParams`, `WorkspaceEdit` with `changes`, `TextEdit`)
- Test: `tests/WpfXaml.Analysis.Tests/RenameTests.cs`, a server test

**Interfaces:**
- Consumes: `ReferenceService.FindNameTarget`, `ReferenceService.FindReferences`
- Produces:
  - `public sealed record XamlTextEdit(string? FilePath, LinePosition Start, LinePosition End, string NewText);`
  - `public static TextSpan? RenameService.PrepareRename(XamlSemanticModel model, int offset)`: the span of the name under the caret, or null when it isn't an `x:Key`/`x:Name`.
  - `public static RenameResult RenameService.Rename(XamlSemanticModel model, int offset, string newName, IReadOnlyList<XamlSemanticModel> otherDocuments)` with `public sealed record RenameResult(IReadOnlyList<XamlTextEdit> Edits, string? Error);`. `Error` is non-null means nothing gets applied, and the server returns an LSP error with that message.

Name rules: `x:Name` must be a valid C# identifier and not a keyword (`SyntaxFacts.GetKeywordKind(...) == None` and `SyntaxFacts.IsValidIdentifier`). `x:Key` must be non-empty, with no whitespace, quotes, `{`, `}` or `,`.

- [ ] **Step 1: Write failing tests**
  - `Rename_ResourceKeyUpdatesAllUses`: the edits cover the definition and both uses, with new text `Brand`.
  - `Rename_ElementNameUpdatesBindings`: `x:Name` and `ElementName=` are both edited.
  - `Rename_InvalidNamesRejected` (Review Focus 4): `""`, `"a b"`, `"a\"b"` and (for x:Name) `"class"` each give a non-null `Error` and no edits.
  - `PrepareRename_OnTypeNameReturnsNull`: the caret on `Button` returns null.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement the service and handlers.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with the message `Add rename for x:Key and x:Name`.

### Task 6: x:Name uses in code-behind

**Files:**
- Create: `src/WpfXaml.Analysis/Features/CodeBehindNames.cs`
- Modify: `ReferenceService.cs`, `RenameService.cs` (include these locations for `XamlNameKind.ElementName` with namescope = document root)
- Test: `tests/WpfXaml.Analysis.Tests/CodeBehindNameTests.cs`

**Interfaces:**
- Consumes: `XamlSemanticModel.ClassSymbol`, `XamlTypeSystem.Compilation`
- Produces: `public static IReadOnlyList<XamlLocation> CodeBehindNames.FindUses(XamlSemanticModel model, string name)`

Rule (user decision, spec A3): in every source declaration of `model.ClassSymbol` that isn't in a generated file (`*.g.cs`, `*.g.i.cs`), take each `IdentifierNameSyntax` whose text equals `name` and that is either a bare simple name or the right side of `this.`. Count it when `GetSymbolInfo` gives no symbol (and no candidates), or gives a field of the class declared in a generated file. Skip it when it binds to anything else (local, parameter, another member, another type). Use Roslyn's semantic model, not text search. Only names on the document root's namescope reach code-behind (template names have no generated field).

- [ ] **Step 1: Write failing tests.** Build the type system with `TestWorkspace.CreateTypeSystem(source)`, where the code-behind uses `okButton`:
  - `CodeBehind_UnboundUseFound`: `okButton.IsEnabled = false;` and `this.okButton.Focus();` give 2 locations in `C:/TestApp/MainWindow.xaml.cs`.
  - `CodeBehind_SameNamedLocalIgnored`: `var okButton = 1; okButton++;` in another method gives 0 for those lines.
  - `CodeBehind_OtherMemberIgnored`: a real property `okButton` declared in the class's non-generated source gives 0.
  - `Rename_ElementNameAlsoEditsCodeBehind`: the rename edits include the `.xaml.cs` locations.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement it, and wire it into references and rename.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with the message `Rename and find x:Name uses in code-behind`.

### Task 7: Formatting (`textDocument/formatting`)

**Files:**
- Create: `src/WpfXaml.Analysis/Features/XamlFormatter.cs`
- Modify: `XamlLanguageServer.cs` (`documentFormattingProvider`, handler), `LspTypes.cs` (`DocumentFormattingParams`, `FormattingOptions`)
- Test: `tests/WpfXaml.Analysis.Tests/FormatterTests.cs`

**Interfaces:**
- Produces: `public static IReadOnlyList<(TextSpan Span, string NewText)> XamlFormatter.Format(XamlDocument document, int indentSize, bool insertSpaces)`. It's syntax only. The server maps the result to `TextEdit[]`.

Rules (spec, user decision: no attribute wrapping):
- If `document.Diagnostics` has any error, return no edits.
- A line that starts with a start tag, an end tag or a comment gets its indentation set to element depth × indent unit.
- A line that starts with an attribute (a continuation of a start tag) is indented to the column of the first attribute when that attribute sits on the tag's first line, otherwise to the element's indent plus one unit.
- Whitespace between attributes on the same line collapses to one space. Trailing whitespace on changed lines is removed.
- Never touch: text content, the inside of comments and CDATA, attribute values (including multi-line ones), or anything under an element with `xml:space="preserve"`.
- Keep line endings as they are (`\r\n` stays `\r\n`). Don't add or remove lines.

- [ ] **Step 1: Write failing tests**
  - `Format_ReindentsNested`: misindented nested elements end up at 0/2/4 with `indentSize: 2, insertSpaces: true`.
  - `Format_TabsWhenRequested`: `insertSpaces: false` uses `\t`.
  - `Format_AlignsContinuationAttributes`: `<Button Content="a"\n      Width="5"/>` → `Width` aligned under `Content`.
  - `Format_CollapsesAttributeSpacing`: `<Button  Content="a"    Width="5"/>` → single spaces, attribute order unchanged.
  - `Format_KeepsTextCommentsAndValues`: multi-line `<TextBlock>` text, a multi-line comment and a multi-line attribute value are unchanged.
  - `Format_PreserveSpaceUntouched`: content under `xml:space="preserve"` is unchanged.
  - `Format_CrLfKept` (Review Focus 1): CRLF input gives CRLF output.
  - `Format_Idempotent`: formatting the formatted output yields no edits (run on every test input above).
  - `Format_SyntaxErrorsNoEdits` (Review Focus 2): a document with `<Button Con` returns no edits.
- [ ] **Step 2:** Run them. Expected: FAIL.
- [ ] **Step 3:** Implement the formatter and handler.
- [ ] **Step 4:** Run the tests. Expected: PASS.
- [ ] **Step 5:** Commit with the message `Add document formatting`.

### Task 8: Release workflow, docs, push

**Files:**
- Create: `.github/workflows/release.yml`
- Modify: `src/WpfXaml.LanguageServer/WpfXaml.LanguageServer.csproj` (version `0.2.0`), `README.md`, `CHANGELOG.md`, `docs/architecture.md` (move the shipped items out of "known gaps"), `editors/` (no change)

- [ ] **Step 1:** Write `release.yml`. Trigger on `push: tags: ['v*']` with `permissions: contents: write`, using the same checkout/setup-dotnet actions as `ci.yml` (no new actions). Run `dotnet publish src/WpfXaml.LanguageServer -c Release -o out`, then zip `out/` to `wpf-xaml-ls-${VERSION}.zip` (VERSION = tag without `v`), then `gh release create "$GITHUB_REF_NAME" wpf-xaml-ls-$VERSION.zip --title "$GITHUB_REF_NAME" --notes-file <CHANGELOG section>`. **Not** `--prerelease`.
- [ ] **Step 2:** Run the same publish and zip commands locally. Expected: the zip contains `wpf-xaml-ls.dll`, and `dotnet out/wpf-xaml-ls.dll --version` prints `0.2.0`.
- [ ] **Step 3:** README: a fork note at the top ("Fork of Zurisen/wpf-xaml-language-server with Linux fixes and …"), new features added to the feature list, a Zed section linking `eRgo35`'s extension repo, and a note on `EnableWindowsTargeting`. CHANGELOG: a `0.2.0` section.
- [ ] **Step 4:** Rerun the spike-style probe against `samples/SampleWpfApp` **without** `NUGET_PACKAGES`. Expected: non-empty completions, plus non-empty documentSymbol and formatting responses.
- [ ] **Step 5:** Commit with the message `Release workflow and docs for 0.2.0`, then `git push origin master`. Check that CI passes: `gh run watch`.
- [ ] **Step 6: Ask the user** before `git tag v0.2.0 && git push origin v0.2.0`. After tagging, check that `gh release view v0.2.0` lists the zip and isn't a prerelease.

## Part B — the Zed extension (`~/work/zed-dotnet`, branch `main`)

### Task 9: Language definition and extension skeleton

**Files:**
- Create: `extension.toml`, `Cargo.toml`, `src/lib.rs` (stub that compiles), `languages/xaml/config.toml`, `languages/xaml/highlights.scm`, `languages/xaml/indents.scm`, `languages/xaml/outline.scm`, `LICENSE` (MIT), `.gitignore` (`target/`, `*.wasm`, `grammars/`)

Values:
- `extension.toml`: `id = "wpf-xaml"`, `name = "WPF XAML"`, `version = "0.1.0"`, `schema_version = 1`, `repository` = this repo's GitHub URL (ask the user if it has no remote yet), `[grammars.xml]` with `repository = "https://github.com/tree-sitter-grammars/tree-sitter-xml"`, `commit = "5000ae8f22d11fbe93939b05c1e37cf21117162d"`, `path = "xml"`, and `[language_servers.wpf-xaml-ls]` with `name = "wpf-xaml-ls"`, `languages = ["XAML"]`, `language_ids = { XAML = "xaml" }`.
- `config.toml`: `name = "XAML"`, `grammar = "xml"`, `path_suffixes = ["xaml"]`, `block_comment = ["<!-- ", " -->"]`, brackets `<`/`>` and `"`/`"` with `close = true`.
- Query files: start from `sweetppro/zed-xml` `languages/xml/*.scm` (the XML extension's). Check that repo's license first and keep its notice if it requires one.
- `Cargo.toml`: `crate-type = ["cdylib"]`, dependency `zed_extension_api = "0.7.0"` (already decided by the spec, so no need to ask).

- [ ] **Step 1:** Check whether the Rust target Zed's current API needs (`wasm32-wasip2` for API 0.7, confirm in the zed docs) is installed: `rustup target list --installed`. If it's missing, **ask the user** before `rustup target add`.
- [ ] **Step 2:** Write the files. Run `cargo build --release --target wasm32-wasip2`. Expected: it builds.
- [ ] **Step 3:** Check whether an installed Zed XML extension also claims `.xaml` or a grammar named `xml` (look in `~/.local/share/zed/extensions/installed/*/extension.toml`). Write the result into the README (Task 10).
- [ ] **Step 4:** Commit with the message `Add XAML language for Zed`.

### Task 10: Launch the server

**Files:**
- Modify: `src/lib.rs`
- Create: `README.md`

**Interfaces:**
- Implements `zed::Extension::language_server_command` and `language_server_initialization_options` for id `wpf-xaml-ls`.

Behavior (spec B), first hit wins:
1. `LspSettings::for_worktree("wpf-xaml-ls", worktree)?.binary.path`: run it with `binary.arguments`, or `["--stdio"]` by default.
2. `worktree.which("wpf-xaml-ls")`: run it with `["--stdio"]`.
3. Otherwise: `zed::latest_github_release("eRgo35/wpf-xaml-language-server", GithubReleaseOptions { require_assets: true, pre_release: false })`. Find the asset `wpf-xaml-ls-{version}.zip`, download it to `wpf-xaml-ls-{version}/` with `DownloadedFileType::Zip` if that folder is missing, delete other `wpf-xaml-ls-*` folders, and run `dotnet` (from `worktree.which("dotnet")`) with `[<dir>/wpf-xaml-ls.dll, "--stdio"]`. Cache the resolved path in the extension struct for the session.
   - If the release lookup or download fails, use the newest existing `wpf-xaml-ls-*` folder (Review Focus 5). If there is none, return the error.
   - If `dotnet` isn't found, return `Err("wpf-xaml-ls needs the .NET SDK: `dotnet` was not found on PATH")`.
   - Use `zed::set_language_server_installation_status` for checking and downloading progress.
- `language_server_initialization_options`: return `LspSettings::for_worktree(...).initialization_options` unchanged.

- [ ] **Step 1:** Implement it. Run `cargo build --release --target wasm32-wasip2`. Expected: it builds with no warnings.
- [ ] **Step 2:** Write README.md: what it does, the requirement (.NET SDK), install steps (Zed → Extensions → Install Dev Extension → this folder), settings examples (`lsp.wpf-xaml-ls.binary.path`, `initialization_options` with `solution`/`loadProjects`/`frameworkVersion`), the WPF-on-Linux csproj note (`<EnableWindowsTargeting>true</EnableWindowsTargeting>`), the XML-extension finding from Task 9, and that the preview isn't possible yet (Zed has no extension panel API).
- [ ] **Step 3:** Commit with the message `Launch wpf-xaml-ls from the extension`, then push.

### Task 11: End-to-end check and cleanup

- [ ] **Step 1:** Create a throwaway WPF project in the scratchpad (`dotnet new wpf` with `-p:EnableWindowsTargeting=true`, adding `<EnableWindowsTargeting>true</EnableWindowsTargeting>` to the csproj), then run `dotnet build`. Expected: build succeeds (it targets Windows; it isn't run).
- [ ] **Step 2:** Ask the user to install the dev extension and open that project in Zed, then check: completion after `<Button `, hover, an error squiggle on a typo, go-to-definition on `x:Class`, find-references and rename on an `x:Name` used in code-behind, Format, and the outline panel. Also, from the user's view: with `wpf-xaml-ls` not on PATH, the server downloads from the v0.2.0 release. Fix anything they report.
- [ ] **Step 3:** Delete the scratchpad `spike/`, `src/` and the throwaway project. Update the spec status to "implemented". Commit and push.
