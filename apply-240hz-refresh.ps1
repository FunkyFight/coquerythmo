$ErrorActionPreference = 'Stop'

function Read-Text([string]$Path) {
    if (!(Test-Path $Path)) {
        throw "File not found: $Path. Run this script from the repository root."
    }
    return [System.IO.File]::ReadAllText((Resolve-Path $Path))
}

function Write-Text([string]$Path, [string]$Text) {
    $utf8NoBom = [System.Text.UTF8Encoding]::new($false)
    [System.IO.File]::WriteAllText((Resolve-Path $Path), $Text, $utf8NoBom)
}

function Newline-Of([string]$Text) {
    if ($Text.Contains("`r`n")) { return "`r`n" }
    return "`n"
}

function Replace-FirstRegex([string]$Text, [string]$Pattern, [scriptblock]$Replacement, [string]$Description) {
    $regex = [System.Text.RegularExpressions.Regex]::new($Pattern)
    $match = $regex.Match($Text)
    if (!$match.Success) {
        throw "Could not find insertion/replacement point for: $Description"
    }
    return $regex.Replace(
        $Text,
        [System.Text.RegularExpressions.MatchEvaluator]{ param($m) & $Replacement $m },
        1
    )
}

function Remove-FirstRegexIfPresent([string]$Text, [string]$Pattern, [scriptblock]$Replacement) {
    $regex = [System.Text.RegularExpressions.Regex]::new($Pattern)
    $match = $regex.Match($Text)
    if (!$match.Success) { return $Text }
    return $regex.Replace(
        $Text,
        [System.Text.RegularExpressions.MatchEvaluator]{ param($m) & $Replacement $m },
        1
    )
}

# 1) constants.rs: add the 240 Hz app-refresh constants.
$constantsPath = 'src/constants.rs'
$constants = Read-Text $constantsPath
$nl = Newline-Of $constants
if ($constants -notmatch 'APP_REFRESH_HZ') {
    $insert = @(
        'pub const DEFAULT_EXPORT_FPS: u32 = 60;',
        '',
        '/// Target app redraw cadence while playback or UI animation is active.',
        '///',
        '/// This only controls interactive app refresh. MP4 export remains controlled',
        '/// independently by `DEFAULT_EXPORT_FPS` / the export modal FPS.',
        'pub const APP_REFRESH_HZ: u64 = 240;',
        'pub const APP_REFRESH_INTERVAL_NS: u64 = 1_000_000_000 / APP_REFRESH_HZ;'
    ) -join $nl
    $constants = Replace-FirstRegex $constants 'pub const DEFAULT_EXPORT_FPS: u32 = 60;' { param($m) $insert } 'DEFAULT_EXPORT_FPS in src/constants.rs'
    Write-Text $constantsPath $constants
}

# 2) state.rs: throttle continuous redraw to APP_REFRESH_HZ instead of the old ~60 Hz deadline.
$statePath = 'src/state.rs'
$state = Read-Text $statePath
$nl = Newline-Of $state

if ($state -notmatch 'fn app_refresh_interval\(') {
    $method = @(
        '    fn app_refresh_interval() -> Duration {',
        '        Duration::from_nanos(constants::APP_REFRESH_INTERVAL_NS)',
        '    }',
        ''
    ) -join $nl
    $state = Replace-FirstRegex $state '(?m)^    fn scroll_decode_due\(&self, now: Instant\) -> bool \{' { param($m) $method + $m.Value } 'scroll_decode_due in src/state.rs'
}

if ($state -notmatch 'fn continuous_redraw_due\(') {
    $method = @(
        '    fn continuous_redraw_due(&self, now: Instant) -> bool {',
        '        (self.needs_continuous_redraw() || self.secondary_needs_continuous_redraw())',
        '            && now.saturating_duration_since(self.last_redraw) >= Self::app_refresh_interval()',
        '    }',
        ''
    ) -join $nl
    $state = Replace-FirstRegex $state '(?m)^    pub fn needs_redraw_now\(&self\) -> bool \{' { param($m) $method + $m.Value } 'needs_redraw_now in src/state.rs'
}

if ($state -notmatch 'self\.continuous_redraw_due\(now\)') {
    $old = 'self.scroll_decode_due\(now\)\r?\n\s*\|\| self.periodic_redraw_due\(now\)\r?\n\s*\|\| self.waveform_redraw_pending\(\)'
    $state = Replace-FirstRegex $state $old {
        param($m)
        @(
            'self.scroll_decode_due(now)',
            '            || self.periodic_redraw_due(now)',
            '            || self.waveform_redraw_pending()',
            '            || self.continuous_redraw_due(now)'
        ) -join $nl
    } 'needs_redraw_now body in src/state.rs'
}

$state = $state.Replace('push_deadline(now + Duration::from_millis(16));', 'push_deadline(self.last_redraw + Self::app_refresh_interval());')
Write-Text $statePath $state

# 3) main.rs: stop immediately re-requesting redraws from inside RedrawRequested.
#    AboutToWait + next_wake_deadline now owns the 240 Hz cadence.
$mainPath = 'src/main.rs'
$main = Read-Text $mainPath

$main = Remove-FirstRegexIfPresent $main '(?s)(WindowEvent::RedrawRequested => \{\r?\n\s*state\.render_secondary_display\(window_id\);\r?\n)\s*if state\.secondary_needs_continuous_redraw\(\) \{\r?\n\s*state\.request_secondary_redraw\(\);\r?\n\s*\}\r?\n' { param($m) $m.Groups[1].Value }

$main = Remove-FirstRegexIfPresent $main '(?s)(WindowEvent::RedrawRequested => \{\r?\n\s*state\.render\(\);\r?\n)\s*if state\.needs_continuous_redraw\(\) \{\r?\n\s*state\.request_redraw\(\);\r?\n\s*\}\r?\n' { param($m) $m.Groups[1].Value }

$main = Remove-FirstRegexIfPresent $main '(?s)(\s*\}) else if state\.secondary_needs_continuous_redraw\(\) \{\r?\n\s*state\.request_secondary_redraw\(\);\r?\n\s*\}' { param($m) $m.Groups[1].Value }

Write-Text $mainPath $main

Write-Host 'Applied 240 Hz app refresh changes. Next: cargo fmt; cargo test'
