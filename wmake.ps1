# wmake.ps1

# Buildfile for windows
$root = Get-Location


# Building Lib
Set-Location .\lib
cargo build --release
Set-Location $root

# Building Test Plugin
Set-Location .\sample_plugin
cargo build --release
Set-Location $root
New-Item plugins -ItemType Directory -Force
Copy-Item .\target\release\sample_plugin.dll .\plugins\ -Force

# Building and running main
cargo run --release
