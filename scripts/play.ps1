param([Parameter(ValueFromRemainingArguments = $true)][string[]]$GameArgs)

$ErrorActionPreference = 'Stop'
$workspaceRoot = Split-Path -Parent $PSScriptRoot
$previousArtifacts = $env:IW4L_ARTIFACTS_DIR
Push-Location -LiteralPath $workspaceRoot
try {
    & cargo build --locked --profile play -p launcher -p iw4l-master --bins -j 2
    if ($LASTEXITCODE -ne 0) { throw 'The optimized game build failed.' }
    if (!$env:IW4L_ARTIFACTS_DIR) {
        $env:IW4L_ARTIFACTS_DIR = Join-Path $workspaceRoot 'iw4l-artifacts'
    }
    & (Join-Path $workspaceRoot 'target/play/iw4l.exe') @GameArgs
    $gameExitCode = $LASTEXITCODE
} finally {
    $env:IW4L_ARTIFACTS_DIR = $previousArtifacts
    Pop-Location
}
exit $gameExitCode
