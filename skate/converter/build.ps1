# Builds iw4l-skate-convert.exe from the pinned SK8-ENGINE source. By default
# it fetches only the tools subtree into a temporary sparse checkout and removes
# it after the build. Pass -SkateEngine to reuse an existing checkout instead.
#
#   pwsh skate/converter/build.ps1
#   pwsh skate/converter/build.ps1 -Out .\dist\windows\skate
#   pwsh skate/converter/build.ps1 -SkateEngine <checkout> -Out <folder>
#
# Needs Python 3.13+ with pyinstaller, numpy and Pillow, plus rustc and git.
param(
    [string] $SkateEngine,
    [string] $Out
)
$ErrorActionPreference = 'Stop'

$repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
if (-not $Out) {
    $Out = Join-Path $repo 'target\play\skate'
}
$Out = [IO.Path]::GetFullPath($Out)

foreach ($tool in 'git', 'rustc') {
    if (-not (Get-Command $tool -ErrorAction SilentlyContinue)) {
        throw "Required build tool '$tool' was not found on PATH"
    }
}
$pythonCommand = Get-Command python -ErrorAction SilentlyContinue
$pythonLauncher = Get-Command py -ErrorAction SilentlyContinue
$python = $null
$pythonArgs = @()
if ($pythonCommand -and $pythonCommand.Source -notmatch '[\\/]Microsoft[\\/]WindowsApps[\\/]') {
    $python = $pythonCommand.Source
} elseif ($pythonLauncher) {
    $python = $pythonLauncher.Source
    $pythonArgs = @('-3')
} elseif ($pythonCommand) {
    $python = $pythonCommand.Source
} else {
    throw 'Python 3.13+ was not found. Install Python and make it available as python or with the Windows py launcher.'
}
& $python @pythonArgs -c "import sys; raise SystemExit(0 if sys.version_info >= (3, 13) else 1)"
if ($LASTEXITCODE -ne 0) { throw "Python 3.13 or newer is required; selected interpreter: $python" }
& $python @pythonArgs -c 'import PyInstaller, numpy, PIL'
if ($LASTEXITCODE -ne 0) {
    throw "Install converter dependencies for the selected Python with: $python $($pythonArgs -join ' ') -m pip install pyinstaller numpy Pillow"
}

New-Item -ItemType Directory -Force $Out | Out-Null
$work = Join-Path ([IO.Path]::GetTempPath()) ("iw4l-skate-convert-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force $work | Out-Null

try {
    if ($SkateEngine) {
        $SkateEngine = (Resolve-Path $SkateEngine).Path
        $revision = (& git -C $SkateEngine rev-parse --short HEAD).Trim()
        if ($LASTEXITCODE -ne 0) { throw "Could not read the Skate engine revision at $SkateEngine" }
        if ($revision -ne 'cb79689') { Write-Warning "Skate engine checkout is at $revision, skate/crates came from cb79689" }
    } else {
        $SkateEngine = Join-Path $work 'engine'
        & git clone --filter=blob:none --no-checkout --sparse `
            https://github.com/SK8-ENGINE/skate-3-rust-engine.git $SkateEngine
        if ($LASTEXITCODE -ne 0) { throw 'Could not clone the Skate engine source' }
        & git -C $SkateEngine sparse-checkout set tools
        if ($LASTEXITCODE -ne 0) { throw 'Could not configure the sparse Skate engine checkout' }
        & git -C $SkateEngine checkout cb79689
        if ($LASTEXITCODE -ne 0) { throw 'Could not check out the pinned Skate engine revision cb79689' }
    }

    $build = Join-Path $work 'build'
    New-Item -ItemType Directory -Force $build | Out-Null
    & rustc --edition 2024 --crate-type cdylib -C opt-level=3 -C panic=abort -C target-feature=+crt-static `
        (Join-Path $SkateEngine 'tools\asset_pipeline\refpack_native.rs') -o (Join-Path $work 'refpack.dll')
    if ($LASTEXITCODE -ne 0) { throw 'refpack.dll build failed' }

    # The converters' sources, without editor-only authoring scripts or tests.
    $tools = Join-Path $SkateEngine 'tools'
    foreach ($source in Get-ChildItem -LiteralPath $tools -File -Recurse) {
        if ($source.FullName -match '[\\/]__pycache__[\\/]') { continue }
        if ($source.Extension -notin '.py','.json','.txt','.md','.toml' -and $source.Name -ne 'LICENSE') { continue }
        $relative = $source.FullName.Substring($tools.Length).TrimStart('\','/').Replace('\', '/')
        if ($relative -like 'mixamo_to_skate/*.json' -or $relative -match '(^|/)test_[^/]*\.py$' -or
            $relative -match '(^|/)blender[^/]*(/|$)') { continue }
        $destination = Join-Path (Join-Path $work 'tools') $relative
        New-Item -ItemType Directory -Force (Split-Path -Parent $destination) | Out-Null
        Copy-Item -LiteralPath $source.FullName -Destination $destination
    }

    & $python @pythonArgs -m PyInstaller --noconfirm --clean --onefile --console --name iw4l-skate-convert `
        --paths $SkateEngine `
        --hidden-import numpy --hidden-import PIL.Image `
        --add-binary "$(Join-Path $work 'refpack.dll');tools/asset_pipeline" `
        --add-data "$(Join-Path $work 'tools');tools" `
        --exclude-module bpy --exclude-module mathutils --exclude-module tkinter `
        --copy-metadata numpy --copy-metadata Pillow `
        --distpath $Out --workpath (Join-Path $work 'pyinstaller-build') --specpath $work `
        (Join-Path $PSScriptRoot 'iw4l_skate_convert.py')
    if ($LASTEXITCODE -ne 0) { throw "PyInstaller failed; ensure $python has PyInstaller, NumPy, and Pillow installed" }
}
finally {
    try {
        Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction Stop
    } catch {
        Write-Warning "Could not remove temporary converter build files at ${work}: $_"
    }
}
