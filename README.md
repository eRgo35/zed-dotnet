# WPF XAML for Zed

Zed support for WPF `.xaml` files: syntax highlighting, plus a language server
([wpf-xaml-ls](https://github.com/eRgo35/wpf-xaml-language-server)) that gives you
Visual Studio–style completion for controls, properties, values and resources, along with:

- hover
- diagnostics
- go to definition
- find references
- rename (also covers `x:Name` uses in code-behind)
- formatting
- the outline

Works on Linux. There is no visual preview, because WPF doesn't run outside Windows.

## Requirements

- The .NET 10 SDK, with `dotnet` on your `PATH`. Your project can target an older .NET.
- A WPF project that restores on Linux. Add this to the `.csproj`:

  ```xml
  <EnableWindowsTargeting>true</EnableWindowsTargeting>
  ```

  Then run `dotnet restore` once, so the WPF reference assemblies
  (`Microsoft.WindowsDesktop.App.Ref`) end up in your NuGet cache.

## Install (dev extension)

1. Clone this repo.
2. In Zed: command palette → `zed: install dev extension` → pick the folder.

On first use, the extension downloads the latest server release from GitHub. If you're
offline, it falls back to the last version it downloaded.

## Settings

All settings are optional and go in `settings.json`:

```jsonc
{
  "lsp": {
    "wpf-xaml-ls": {
      // Use your own server build instead of the downloaded one.
      "binary": { "path": "/path/to/wpf-xaml-ls", "arguments": ["--stdio"] },
      "initialization_options": {
        "solution": "MyApp.sln",   // .sln/.slnx/.csproj to load; default: found from the workspace root
        "loadProjects": true,      // false: framework types only, no MSBuild
        "frameworkVersion": 8      // WPF targeting pack major version; default: highest installed
      }
    }
  }
}
```

The server is found in this order:

1. `binary.path` from your settings.
2. A `wpf-xaml-ls` on your `PATH`.
3. The downloaded release, run with `dotnet`.

## Notes

- Zed's XML extension doesn't claim `.xaml`, so the two don't conflict. If another
  extension does claim it, add `"file_types": { "XAML": ["xaml"] }` to your settings.
- The tree-sitter XML grammar and queries come from
  [zed-xml](https://github.com/sweetppro/zed-xml) (MIT).

## License

MIT
