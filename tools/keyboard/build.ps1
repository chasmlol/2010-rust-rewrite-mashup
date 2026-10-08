param(
    [Parameter(Mandatory = $true)]
    [string]$DependencyDirectory,
    [string]$OutputDirectory = (Join-Path $PSScriptRoot 'bin')
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
$taskDependencies = (Resolve-Path -LiteralPath $DependencyDirectory).Path
$taskCompiler = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
if (-not (Test-Path -LiteralPath $taskCompiler)) {
    throw 'The Windows .NET Framework C# compiler was not found.'
}
$taskDlls = @('INIFileParser.dll', 'log4net.dll', 'Nefarius.ViGEmClient.dll')
foreach ($taskDll in $taskDlls) {
    if (-not (Test-Path -LiteralPath (Join-Path $taskDependencies $taskDll))) {
        throw "Missing dependency: $taskDll"
    }
}

New-Item -ItemType Directory -Path $OutputDirectory -Force | Out-Null
$taskOutput = (Resolve-Path -LiteralPath $OutputDirectory).Path
$taskKeyboard = Join-Path $taskOutput 'keyboard'
New-Item -ItemType Directory -Path $taskKeyboard -Force | Out-Null
foreach ($taskDll in $taskDlls) {
    Copy-Item -LiteralPath (Join-Path $taskDependencies $taskDll) -Destination $taskKeyboard -Force
}
$taskSources = Join-Path $PSScriptRoot 'source'
$taskLibrary = Join-Path $taskKeyboard 'Keyboard2XinputLib.dll'
$taskLibraryArgs = @('/nologo', '/target:library', '/platform:anycpu', "/out:$taskLibrary", '/reference:System.Windows.Forms.dll')
foreach ($taskDll in $taskDlls) {
    $taskLibraryArgs += '/reference:' + (Join-Path $taskKeyboard $taskDll)
}
foreach ($taskFile in @('AssemblyInfo.cs', 'Config.cs', 'Keyboard2Xinput.cs', 'StateListener.cs', 'ViGEmBusNotFoundException.cs')) {
    $taskLibraryArgs += Join-Path $taskSources $taskFile
}
& $taskCompiler @taskLibraryArgs
if ($LASTEXITCODE -ne 0) { throw 'Keyboard controller library compilation failed.' }

& $taskCompiler /nologo /target:winexe /platform:x86 "/out:$(Join-Path $taskKeyboard 'Keyboard2XinputGui.exe')" /reference:System.Windows.Forms.dll /reference:System.Drawing.dll "/reference:$taskLibrary" (Join-Path $taskSources 'NumpadHost.cs')
if ($LASTEXITCODE -ne 0) { throw 'Keyboard controller host compilation failed.' }

& $taskCompiler /nologo /target:winexe /platform:x86 "/out:$(Join-Path $taskOutput 'MW2 Skate Keyboard Launcher.exe')" /reference:System.Windows.Forms.dll (Join-Path $taskSources 'KeyboardLaunch.cs')
if ($LASTEXITCODE -ne 0) { throw 'Game launcher compilation failed.' }

# Keep any existing user bindings when rebuilding.
$taskMapping = Join-Path $taskKeyboard 'mapping.ini'
if (-not (Test-Path -LiteralPath $taskMapping)) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'mapping.default.ini') -Destination $taskMapping
}
Copy-Item -LiteralPath (Join-Path $taskSources 'LICENSE.txt') -Destination (Join-Path $taskKeyboard 'LICENSE-Keyboard2Xinput.txt') -Force
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Play Rust.cmd') -Destination $taskOutput -Force
Write-Output "Built keyboard support in $taskOutput. Copy its contents beside your built iw4l.exe; existing mapping.ini files should be preserved."
