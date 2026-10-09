# Install ctx (Windows).
#   irm https://raw.githubusercontent.com/Idrisanu/ctx/main/install.ps1 | iex
$ErrorActionPreference = "Stop"

$Repo = "Idrisanu/ctx"
$BinDir = if ($env:CTX_BIN_DIR) { $env:CTX_BIN_DIR } else { "$env:USERPROFILE\.local\bin" }
$Target = "x86_64-pc-windows-msvc"

$Version = if ($env:CTX_VERSION) { $env:CTX_VERSION } else { "latest" }
if ($Version -eq "latest") {
  $Url = "https://github.com/$Repo/releases/latest/download/ctx-$Target.zip"
} else {
  $Url = "https://github.com/$Repo/releases/download/$Version/ctx-$Target.zip"
}

$tmp = Join-Path $env:TEMP ("ctx-" + [Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Path $tmp | Out-Null
try {
  Write-Host "downloading $Url"
  Invoke-WebRequest -Uri $Url -OutFile (Join-Path $tmp "ctx.zip")
  Expand-Archive -Path (Join-Path $tmp "ctx.zip") -DestinationPath $tmp
  New-Item -ItemType Directory -Force -Path $BinDir | Out-Null
  Move-Item -Force (Join-Path $tmp "ctx.exe") (Join-Path $BinDir "ctx.exe")
  Write-Host "installed ctx to $BinDir\ctx.exe"
  if (($env:PATH -split ";") -notcontains $BinDir) {
    Write-Host "NOTE: $BinDir is not on your PATH. Add it via System Properties > Environment Variables."
  }
  & (Join-Path $BinDir "ctx.exe") --version
} finally {
  Remove-Item -Recurse -Force $tmp -ErrorAction SilentlyContinue
}
