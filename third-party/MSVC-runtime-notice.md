# Microsoft Visual C++ runtime

`tools/vcruntime140.dll` is an unmodified Microsoft runtime copied from the
installed Visual Studio 2026 `VC/Redist/MSVC/14.51.36231/x64/Microsoft.VC145.CRT`
directory. It is deployed beside Typst, without a system-wide installation.
Microsoft's terms apply to this component; it is not covered by the application's
own source license. Copyright Microsoft Corporation.

- Redistribution list: https://learn.microsoft.com/en-us/visualstudio/releases/2026/redistribution
- Local deployment: https://learn.microsoft.com/en-us/cpp/windows/choosing-a-deployment-method
- License terms: https://visualstudio.microsoft.com/license-terms/

Future releases must update this bundled component when needed; application-local
copies are maintained by the application distributor.
