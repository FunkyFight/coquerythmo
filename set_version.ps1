$version = $args[0]
$file = "Cargo.toml"
$content = Get-Content $file -Raw
$content = $content -replace '(?m)^version = "[^"]*"', "version = `"$version`""
Set-Content $file $content -NoNewline
Write-Host "Version set to $version"
