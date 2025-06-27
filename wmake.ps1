# wmake.ps1

# Buildfile for windows
$root = Get-Location


# Building Lib
Set-Location .\lib
cargo build
Set-Location $root

# Building Test Plugin
Set-Location .\test_plugin
cargo build
Set-Location $root
New-Item Plugins -ItemType Directory -Force
Copy-Item .\target\debug\sample_plugin.dll .\Plugins\ -Force

# Building and running main
cargo run -- --local-dev --l DEBUG
