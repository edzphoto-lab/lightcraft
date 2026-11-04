<# Personal Windows x64 package. Run only in a fresh, disposable build checkout. #>
param([Parameter(Mandatory = $true)] [ValidateSet('Prepare', 'Package')] [string] $Stage)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
Set-Location $root
$version = $env:LIGHTCRAFT_VERSION
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'This script modifies build inputs; run it in GitHub Actions, not a working checkout.' }
if ($version -notmatch '^\d+\.\d+\.\d+$') { throw 'A simple numeric EdZ version is required.' }
$name = "LightCraft EdZ Lab_$version"
$env:LIGHTCRAFT_EDZ_LAB = '1'
$env:DIST = Join-Path $root 'dist\release'
New-Item -ItemType Directory -Force $env:DIST | Out-Null
$tools = Join-Path $env:RUNNER_TEMP 'edz-node-tools'
function Native([string] $what, [scriptblock] $run) {
  Write-Output "==> $what"
  & $run
  if ($LASTEXITCODE -ne 0) { throw "$what exited $LASTEXITCODE" }
}
if ($Stage -eq 'Prepare') {
  Native 'Install pinned icon renderer' { npm install --prefix $tools --no-audit --no-fund @resvg/resvg-js@2.6.2 }
}
$env:NODE_PATH = Join-Path $tools 'node_modules'
$transcript = Join-Path $env:DIST 'validation.txt'
Start-Transcript -Path $transcript -Append | Out-Null
try {
  if ($Stage -eq 'Prepare') {
    Native 'Generate red application icons' { node packaging/windows/edz-icons.cjs }
    # Separate upgrade identity for EACH version: installing it cannot upgrade/remove another release.
    $hash = [Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes("org.edzphoto.lightcraft.windows.$version"))
    $upgrade = ([guid]::new([byte[]]$hash[0..15])).ToString().ToUpperInvariant()
    $componentHash = [Security.Cryptography.SHA256]::HashData([Text.Encoding]::UTF8.GetBytes("org.edzphoto.lightcraft.windows.app.$version"))
    $component = ([guid]::new([byte[]]$componentHash[0..15])).ToString().ToUpperInvariant()
    $wxsPath = Join-Path $PSScriptRoot 'lightcraft.wxs'
    $wxs = [IO.File]::ReadAllText($wxsPath)
    $wxs = $wxs.Replace('229BB552-5EFD-479A-BA86-8CB6D926390C', $upgrade).Replace('8A1D3A39-F7C1-47B1-97AE-40C80BF82F4A', $component)
    $wxs = $wxs.Replace('Name="LightCraft"', "Name=`"$name`"")
    $wxs = $wxs.Replace('Software\LightCraft', "Software\$name").Replace('App Paths\lightcraft.exe', "App Paths\$name.exe")
    $wxs = $wxs.Replace('Learning Machines LLC', 'EdZ Lab').Replace('https://getartcraft.com/apps/lightcraft', 'https://github.com/edzphoto-lab/lightcraft')
    $wxs = $wxs.Replace('Launch LightCraft', "Launch $name").Replace('LightCraft has been installed.', "$name has been installed.")
    [IO.File]::WriteAllText($wxsPath, $wxs)
    $policyPath = Join-Path $PSScriptRoot 'install-policy.wxi'
    $policy = [IO.File]::ReadAllText($policyPath).Replace('Software\LightCraft', "Software\$name").Replace('App Paths\lightcraft.exe', "App Paths\$name.exe").Replace('\ArtCraft\LightCraft\', "\ArtCraft\$name\")
    [IO.File]::WriteAllText($policyPath, $policy)
    $jsonPath = Join-Path $PSScriptRoot 'install-policy.json'
    $json = Get-Content -Raw $jsonPath | ConvertFrom-Json
    $json.name = $name
    $json.upgradeCode = $upgrade
    foreach ($entry in $json.registry) {
      $entry[1] = $entry[1].Replace('Software\LightCraft', "Software\$name").Replace('App Paths\lightcraft.exe', "App Paths\$name.exe")
    }
    $json | ConvertTo-Json -Depth 10 | Set-Content -Encoding utf8 $jsonPath
    $buildPath = Join-Path $root 'apps\lightcraft\build.rs'
    $build = [IO.File]::ReadAllText($buildPath).Replace('"ProductName", "LightCraft"', "`"ProductName`", `"$name`"").Replace('"FileDescription", "LightCraft photo library and raw developer"', "`"FileDescription`", `"$name`"")
    [IO.File]::WriteAllText($buildPath, $build)

    Write-Output "PASS prepared $name Windows build inputs."
    return
  }
  Native 'Install WiX 5.0.2' { dotnet tool install --global wix --version 5.0.2 }
  Native 'Build and package x64' { & packaging/windows/package.ps1 -Arch x64 }
  foreach ($file in Get-ChildItem $env:DIST -File | Where-Object Extension -in '.msi', '.zip') {
    $newName = $file.Name.Replace('lightcraft-', 'lightcraft-edz-lab-')
    Rename-Item -LiteralPath $file.FullName -NewName $newName
  }
  $msi = (Get-ChildItem $env:DIST -Filter '*.msi').FullName
  $installDir = Join-Path $env:LOCALAPPDATA "Programs\ArtCraft\$name"
  $installLog = Join-Path $env:DIST 'install.log'
  $p = Start-Process msiexec.exe -ArgumentList @('/i', "`"$msi`"", '/qn', 'ALLUSERS=2', 'MSIINSTALLPERUSER=1', "INSTALLFOLDER=`"$installDir`"", '/l*v', "`"$installLog`"") -Wait -PassThru
  if ($p.ExitCode -ne 0) { throw "MSI install failed: $($p.ExitCode)" }
  try {
    $exe = Join-Path $installDir 'lightcraft.exe'
    $cli = Join-Path $installDir 'lightcraft-cli.exe'
    if (-not (Test-Path $exe)) { throw 'The installer did not install the application.' }
    $info = (Get-Item $exe).VersionInfo
    if ($info.ProductName -ne $name) { throw "Wrong application identity: $($info.ProductName)" }
    $cliVersion = & $cli --version | Out-String
    if ($LASTEXITCODE -ne 0 -or $cliVersion -notmatch [regex]::Escape($version)) { throw "Wrong CLI version: $cliVersion" }
    $shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) "$name.lnk"
    if (-not (Test-Path $shortcut)) { throw 'Missing versioned Start menu shortcut.' }
    Write-Output "PASS installed $name, x64 GUI executable, versioned shortcut and CLI: $cliVersion"
    # Test actual photo rendering using the installed CLI, without touching a real catalogue.
    Native 'Headless render from installed application' { & $cli snapshot --demo -o (Join-Path $env:DIST 'windows-preview.png') --size 1280x800 }
  } finally {
    $p = Start-Process msiexec.exe -ArgumentList @('/x', "`"$msi`"", '/qn') -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "MSI uninstall failed: $($p.ExitCode)" }
    if (Test-Path (Join-Path $installDir 'lightcraft.exe')) { throw 'Uninstall left the application behind.' }
    Write-Output 'PASS uninstall removed the test application.'
  }
  $hashLines = foreach ($file in Get-ChildItem $env:DIST -File | Where-Object Extension -in '.msi', '.zip') {
    "$((Get-FileHash -Algorithm SHA256 $file.FullName).Hash.ToLowerInvariant())  $($file.Name)"
  }
  $hashLines | Set-Content -Encoding ascii (Join-Path $env:DIST 'SHA256SUMS.txt')
  Write-Output "PASS $name Windows x64 installer and portable ZIP ready. External-editor automatic discovery remains macOS-only; Windows supports manually selected executables. Unsigned personal build."
} finally {
  Stop-Transcript | Out-Null
  # Uploads run as an Actions step: shell commands do not receive the runtime artifact token.
}
