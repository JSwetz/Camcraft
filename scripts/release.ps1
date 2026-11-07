# Release cut (Release train, 0.8.0): automates the 0.7.0 recipe from
# RECOMMENDED.md ("Release 0.7.0", "The release train"); the zip layout
# mirrors .github/workflows/release.yml. Never pushes.
# M13.3: signing rides scripts/sign.ps1 (a no-op without a certificate in
# the environment) and -Installer adds the Inno Setup build from the same
# staged tree (skipped by name when ISCC.exe is not installed).
param(
    [Parameter(Mandatory = $true)][string]$Version,
    [string]$Title = "release",
    [switch]$SkipGate,
    [switch]$SkipBuild,
    [switch]$Installer,
    [switch]$DryRun,
    # Branch/PR flow (AGENTS.md "A cut goes PR review -> merge -> tag"):
    # commit the bump on release/vX.Y.Z and build, but leave the tag to
    # main after the release PR merges.
    [switch]$NoTag
)
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot
Set-Location $root

$allCrates = @("camcraft-app", "camcraft-cl", "camcraft-geometry", "camcraft-kernel",
    "camcraft-motion", "camcraft-post", "camcraft-sim", "camcraft-tools", "camcraft-ui")
$nonUi = $allCrates | Where-Object { $_ -ne "camcraft-ui" }
$tag = "v$Version"
$coAuthor = "Co-Authored-By: Claude Fable 5.1 <noreply@anthropic.com>"

function Banner([string]$text) {
    Write-Host ""
    Write-Host "=== $text ==="
}
function Fail([string]$msg) {
    Write-Host "RELEASE FAILED: $msg"
    exit 1
}
function Pkgs([string[]]$names) {
    $out = @()
    foreach ($n in $names) { $out += @("-p", $n) }
    return $out
}
# Run a native command; in dry-run print it instead. Stops on non-zero.
function Run([string]$exe, [string[]]$arguments, [string]$cwd = "") {
    $line = "$exe $($arguments -join ' ')"
    if ($cwd) { $line = "(cd $cwd) $line" }
    if ($DryRun) { Write-Host "[dry-run] $line"; return }
    Write-Host "> $line"
    if ($cwd) { Push-Location $cwd }
    try { & $exe @arguments } finally { if ($cwd) { Pop-Location } }
    if ($LASTEXITCODE -ne 0) { Fail "'$line' exited with $LASTEXITCODE" }
}

if ($Version -notmatch '^\d+\.\d+\.\d+$') { Fail "-Version must be X.Y.Z, got '$Version'" }
if ($DryRun) { Write-Host "DRY RUN: nothing below changes state." }

# ---- 1. Preflight -------------------------------------------------------
Banner "Phase 1/10: preflight"
$allowed = @("RELEASE_NOTES.md", "TESTING.md", "Cargo.toml", "Cargo.lock", "RECOMMENDED.md")
$status = @(git status --porcelain)
if ($LASTEXITCODE -ne 0) { Fail "git status failed" }
$dirty = @()
foreach ($l in $status) {
    if (-not $l) { continue }
    $p = $l.Substring(3).Trim('"')
    if ($allowed -notcontains $p) { $dirty += $p }
}
if ($dirty.Count -gt 0) {
    Write-Host "Unexpected changes:"
    $dirty | ForEach-Object { Write-Host "  $_" }
    if ($DryRun) { Write-Host "[dry-run] would fail here" } else { Fail "working tree not clean" }
} else { Write-Host "tree clean (ignoring release files)" }
$branchNow = (git rev-parse --abbrev-ref HEAD)
if ($LASTEXITCODE -ne 0) { Fail "git rev-parse failed" }
# Branch/PR flow: the bump is committed on release/vX.Y.Z (-NoTag), never on
# main; a tagging run is the legacy direct cut and belongs on main only.
if ($NoTag -and $branchNow -eq "main") { Fail "-NoTag runs on a release/v$Version branch, not on main" }
if (-not $NoTag) { Fail "main is branch-protected (PR only): cut on release/v$Version with -NoTag and tag main after the release PR merges" }
$existing = git tag --list $tag
if ($LASTEXITCODE -ne 0) { Fail "git tag failed" }
if ($existing) { Fail "tag $tag already exists" }
Write-Host "tag $tag is free"

# ---- 2. Gate ------------------------------------------------------------
Banner "Phase 2/10: gate"
if ($SkipGate) { Write-Host "skipped (-SkipGate)" } else {
    Run "cargo" (@("test", "-j", "6") + (Pkgs $nonUi) + @("--no-fail-fast", "--", "--test-threads=6"))
    Run "cargo" @("test", "-j", "6", "-p", "camcraft-ui", "--", "--test-threads=6") (Join-Path $root "crates/ui")
    Run "cargo" (@("clippy", "-j", "6") + (Pkgs $allCrates) + @("--tests", "--", "-D", "warnings"))
    foreach ($c in @("camcraft-motion", "camcraft-sim", "camcraft-app")) {
        $a = @("test", "-j", "6", "--release", "--no-fail-fast", "-p", $c, "--", "--ignored")
        if ($c -ne "camcraft-app") { $a += @("--skip", "hsm_one") }
        $a += "--test-threads=6"
        Run "cargo" $a
    }
}

# ---- 3. Bump ------------------------------------------------------------
Banner "Phase 3/10: version bump"
$tomlPath = Join-Path $root "Cargo.toml"
$toml = [System.IO.File]::ReadAllText($tomlPath)
$m = [regex]::Match($toml, '(?s)(\[workspace\.package\].*?\r?\nversion\s*=\s*")([^"]+)(")')
if (-not $m.Success) { Fail "no version in [workspace.package]" }
$old = $m.Groups[2].Value
Write-Host "workspace version: $old -> $Version"
if ($DryRun) {
    Write-Host "[dry-run] would rewrite Cargo.toml version"
    Write-Host "[dry-run] cargo update -w -q"
    Write-Host "[dry-run] would assert Cargo.lock diff touches only version lines"
} else {
    $new = $toml.Substring(0, $m.Groups[2].Index) + $Version + $toml.Substring($m.Groups[2].Index + $m.Groups[2].Length)
    [System.IO.File]::WriteAllText($tomlPath, $new, (New-Object System.Text.UTF8Encoding($false)))
    Run "cargo" @("update", "-w", "-q")
    $lockDiff = git diff Cargo.lock
    if ($LASTEXITCODE -ne 0) { Fail "git diff Cargo.lock failed" }
    $changed = @($lockDiff | Where-Object { $_ -match '^[-+][^-+]' })
    $verLines = @($lockDiff | Select-String '^[-+]version')
    if ($changed.Count -ne $verLines.Count) { Fail "Cargo.lock changed beyond version lines" }
    Write-Host "Cargo.lock: $($verLines.Count) version lines changed, nothing else"
}

# ---- 4. Commit and tag --------------------------------------------------
Banner "Phase 4/10: commit and tag"
$msg1 = "chore(release): $Version - $Title`n`n$coAuthor"
$msg2 = "chore(release): sync Cargo.lock to $Version`n`n$coAuthor"
if ($DryRun) {
    Write-Host "[dry-run] git add Cargo.toml RELEASE_NOTES.md TESTING.md RECOMMENDED.md (if changed)"
    Write-Host "[dry-run] git commit -m 'chore(release): $Version - $Title' (+ Co-Authored-By)"
    Write-Host "[dry-run] git add Cargo.lock; git commit -m 'chore(release): sync Cargo.lock to $Version'"
    if ($NoTag) { Write-Host "[dry-run] no tag (-NoTag): tag main after the release PR merges" }
    else { Write-Host "[dry-run] git tag -a $tag -m 'CamCraft $Version'" }
} else {
    foreach ($f in @("Cargo.toml", "RELEASE_NOTES.md", "TESTING.md", "RECOMMENDED.md")) {
        if (git status --porcelain -- $f) { git add -- $f; if ($LASTEXITCODE -ne 0) { Fail "git add $f" } }
    }
    Run "git" @("commit", "-m", $msg1)
    Run "git" @("add", "Cargo.lock")
    Run "git" @("commit", "-m", $msg2)
    if ($NoTag) { Write-Host "no tag (-NoTag): tag main after the release PR merges" }
    else { Run "git" @("tag", "-a", $tag, "-m", "CamCraft $Version") }
}

# ---- 5. Build -----------------------------------------------------------
Banner "Phase 5/10: release build"
if ($SkipBuild) { Write-Host "skipped (-SkipBuild)" } else {
    Run "cargo" @("build", "-j", "6", "--release")
}

# ---- 6. Verify binary version --------------------------------------------
Banner "Phase 6/10: verify post.exe version"
$postExe = Join-Path $root "target/release/post.exe"
if ($DryRun) {
    Write-Host "[dry-run] $postExe --version must report $Version"
} else {
    if (-not (Test-Path $postExe)) { Fail "post.exe missing (run without -SkipBuild)" }
    $out = (& $postExe --version)
    if ($LASTEXITCODE -ne 0) { Fail "post.exe --version failed" }
    $reported = ($out -split '\s+')[1]
    if ($reported -ne $Version) { Fail "post.exe reports '$reported', expected $Version" }
    Write-Host "post.exe reports $reported"
}

# ---- 7. Sign ------------------------------------------------------------
# Before staging, so the zip and the installer carry the same signed exes.
# sign.ps1 prints "unsigned: no certificate configured" and exits 0 when no
# CAMCRAFT_SIGN_* variable is set: an unsigned cut is still a cut.
Banner "Phase 7/10: sign binaries"
$exes = @("camcraft-ui.exe", "camcraft-cli.exe", "post.exe") | ForEach-Object { Join-Path $root "target/release/$_" }
$signScript = Join-Path $PSScriptRoot "sign.ps1"
if ($SkipBuild -and -not $DryRun -and -not (Test-Path $exes[0])) {
    Write-Host "skipped (no release exes; -SkipBuild)"
} else {
    Run "powershell" (@("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $signScript, "-Path") + $exes)
}

# ---- 8. Assemble --------------------------------------------------------
Banner "Phase 8/10: assemble zip"
$stage = Join-Path $root "dist/camcraft-$Version-win-x64"
$zip = Join-Path $root "dist/camcraft-$Version-win-x64.zip"
if ($DryRun) {
    Write-Host "[dry-run] stage $stage : exes (camcraft-ui, camcraft-cli, post), assets/{posts,machines,ui,scripts,themes}, RELEASE_NOTES.md, TESTING.md, LICENSE"
    Write-Host "[dry-run] Compress-Archive -Path $stage -DestinationPath $zip -Force"
} else {
    if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
    $dirs = @("assets/posts", "assets/machines", "assets/ui", "assets/scripts", "assets/themes") | ForEach-Object { Join-Path $stage $_ }
    New-Item -ItemType Directory -Force $dirs | Out-Null
    foreach ($e in @("camcraft-ui.exe", "camcraft-cli.exe", "post.exe")) {
        Copy-Item (Join-Path $root "target/release/$e") $stage
    }
    Copy-Item "assets/posts/*" (Join-Path $stage "assets/posts")
    Copy-Item "assets/machines/*" (Join-Path $stage "assets/machines")
    Copy-Item "assets/ui/*" (Join-Path $stage "assets/ui") -Recurse
    Copy-Item "assets/scripts/*" (Join-Path $stage "assets/scripts")
    Copy-Item "assets/themes/*" (Join-Path $stage "assets/themes")
    Copy-Item "RELEASE_NOTES.md", "TESTING.md", "LICENSE" $stage
    Compress-Archive -Path $stage -DestinationPath $zip -Force
}

# ---- 9. Installer (opt-in) ----------------------------------------------
# Same staged tree as the zip; the zip stays the primary test vehicle.
Banner "Phase 9/10: installer"
$setup = Join-Path $root "dist/camcraft-$Version-win-x64-setup.exe"
if (-not $Installer) {
    Write-Host "skipped (pass -Installer to build camcraft-$Version-win-x64-setup.exe with Inno Setup)"
} else {
    $installerScript = Join-Path $PSScriptRoot "installer.ps1"
    Run "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $installerScript, "-Version", $Version, "-Stage", $stage, "-OutDir", (Join-Path $root "dist"))
    if ($DryRun) {
        Write-Host "[dry-run] sign.ps1 -Path $setup (if ISCC produced it)"
    } elseif (Test-Path $setup) {
        Run "powershell" @("-NoProfile", "-ExecutionPolicy", "Bypass", "-File", $signScript, "-Path", $setup)
    } else {
        Write-Host "no installer produced (ISCC.exe missing); the zip is the release"
    }
}

# ---- 10. Summary --------------------------------------------------------
Banner "Phase 10/10: done"
Write-Host "Zip: $zip"
if ($Installer -and (Test-Path $setup)) { Write-Host "Installer: $setup" }
if ($NoTag) {
    $branch = (git rev-parse --abbrev-ref HEAD)
    Write-Host "Next: git push -u origin $branch; gh pr create --base main --title 'chore(release): $Version'"
    Write-Host "      record tip=`$(git rev-parse $branch) before merging; after it merges: git switch main; git pull --ff-only;"
    Write-Host "      git diff `$tip main must be empty, then git tag -a $tag -m 'CamCraft $Version' main; git push origin $tag"
} else {
    Write-Host "main is branch-protected: a direct push is refused. Cut on release/v$Version with -NoTag (AGENTS.md 'A cut goes PR review')."
}
Write-Host "NOTHING WAS PUSHED. Pushing is left to you."
exit 0
