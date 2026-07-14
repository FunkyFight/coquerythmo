$ErrorActionPreference = 'Stop'

# These files are the current domain-facing modules. Keep this check small and
# explicit so it catches accidental UI/platform coupling without becoming a
# second dependency analyser.
$domainFiles = @(
    'src/project.rs',
    'src/rythmo_line.rs',
    'src/rythmo_drawing.rs',
    'src/syllable.rs',
    'src/command.rs'
)

$patterns = @('crate::ui', '\bwinit\b', '\bwgpu\b', 'crate::network')
$violations = @()
foreach ($file in $domainFiles) {
    foreach ($pattern in $patterns) {
        $matches = rg --line-number --color never -- $pattern $file 2>$null
        if ($LASTEXITCODE -eq 0) {
            $violations += "${file}: $matches"
        }
    }
}

if ($violations.Count -gt 0) {
    Write-Error ('Forbidden domain dependency detected:`n' + ($violations -join "`n"))
}

$legacyFiles = @(
    'src/ui/file_explorer_modal.rs',
    'src/ui/rythmo.rs',
    'src/ui/widget.rs',
    'src/video_export.rs'
)
foreach ($file in $legacyFiles) {
    if (Test-Path -LiteralPath $file) {
        Write-Error "Legacy monolith still exists: $file"
    }
}

$legacyReferences = rg --line-number --color never `
    'file_explorer_modal|crate::ui::rythmo|crate::ui::widget|super::widget|ui::scroll_delta_to_frames|ProjectSession::reset' `
    src tests 2>$null
if ($LASTEXITCODE -eq 0) {
    Write-Error ('Legacy refactor path referenced:`n' + $legacyReferences)
}
