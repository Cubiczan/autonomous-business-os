$ErrorActionPreference = "Stop"

$RepoRoot = Split-Path -Parent $PSScriptRoot
$RepoName = "autonomous-business-os"
$Git = "C:\Program Files\Git\cmd\git.exe"
$ResultsPath = Join-Path $RepoRoot "push-results.txt"

Set-Content -Path $ResultsPath -Value "Autonomous Business OS push results`nStarted: $(Get-Date -Format o)`n"

function Add-Result {
    param([string]$Message)
    Write-Host $Message
    Add-Content -Path $ResultsPath -Value $Message
}

function ConvertTo-PlainText {
    param([System.Security.SecureString]$Secure)
    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Secure)
    try {
        [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    }
    finally {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
}

function Read-Pat {
    param([string]$Prompt)
    $secure = Read-Host $Prompt -AsSecureString
    $plain = ConvertTo-PlainText $secure
    if ([string]::IsNullOrWhiteSpace($plain)) {
        return $null
    }
    return $plain
}

function Ensure-Remote {
    param(
        [string]$Name,
        [string]$Url
    )
    & $Git -C $RepoRoot remote get-url $Name *> $null
    if ($LASTEXITCODE -eq 0) {
        & $Git -C $RepoRoot remote set-url $Name $Url
    }
    else {
        & $Git -C $RepoRoot remote add $Name $Url
    }
    if ($LASTEXITCODE -ne 0) {
        throw "Could not configure remote $Name"
    }
}

function Ensure-GitHubRepo {
    param(
        [string]$Owner,
        [string]$Token
    )
    $headers = @{
        Authorization          = "Bearer $Token"
        Accept                 = "application/vnd.github+json"
        "X-GitHub-Api-Version" = "2022-11-28"
    }
    $repoUrl = "https://api.github.com/repos/$Owner/$RepoName"
    try {
        Invoke-RestMethod -Method Get -Uri $repoUrl -Headers $headers *> $null
        Add-Result "GitHub repo exists: $Owner/$RepoName"
    }
    catch {
        $status = $_.Exception.Response.StatusCode.value__
        if ($status -ne 404) {
            throw
        }
        $body = @{
            name        = $RepoName
            private     = $false
            description = "Autonomous Business Operating System with multi-agent orchestration."
        } | ConvertTo-Json
        Invoke-RestMethod -Method Post -Uri "https://api.github.com/user/repos" -Headers $headers -ContentType "application/json" -Body $body *> $null
        Add-Result "Created GitHub repo: $Owner/$RepoName"
    }
}

function Ensure-CodebergRepo {
    param(
        [string]$Owner,
        [string]$Token
    )
    $headers = @{
        Authorization = "token $Token"
        Accept        = "application/json"
    }
    $repoUrl = "https://codeberg.org/api/v1/repos/$Owner/$RepoName"
    try {
        Invoke-RestMethod -Method Get -Uri $repoUrl -Headers $headers *> $null
        Add-Result "Codeberg repo exists: $Owner/$RepoName"
    }
    catch {
        $status = $_.Exception.Response.StatusCode.value__
        if ($status -ne 404) {
            throw
        }
        $body = @{
            name        = $RepoName
            private     = $false
            auto_init   = $false
            description = "Autonomous Business Operating System with multi-agent orchestration."
        } | ConvertTo-Json
        Invoke-RestMethod -Method Post -Uri "https://codeberg.org/api/v1/user/repos" -Headers $headers -ContentType "application/json" -Body $body *> $null
        Add-Result "Created Codeberg repo: $Owner/$RepoName"
    }
}

function Push-WithToken {
    param(
        [string]$RemoteName,
        [string]$RemoteUrl,
        [string]$Username,
        [string]$Token
    )
    Ensure-Remote -Name $RemoteName -Url $RemoteUrl

    $askpass = Join-Path $env:TEMP "business-os-$RemoteName-askpass.cmd"
    Set-Content -Path $askpass -Value @"
@echo off
echo %~1 | findstr /i "Username" >nul
if %errorlevel%==0 (
  echo %GIT_USERNAME%
) else (
  echo %GIT_PASSWORD%
)
"@

    $env:GIT_USERNAME = $Username
    $env:GIT_PASSWORD = $Token
    $env:GIT_ASKPASS = $askpass
    $env:GIT_TERMINAL_PROMPT = "0"
    $env:GCM_INTERACTIVE = "Never"

    try {
        & $Git -C $RepoRoot -c credential.helper= -c core.askpass="$askpass" push -u $RemoteName main
        if ($LASTEXITCODE -ne 0) {
            throw "git push failed for $RemoteName"
        }
        Add-Result "Pushed main to $RemoteName ($RemoteUrl)"
    }
    finally {
        Remove-Item Env:\GIT_USERNAME -ErrorAction SilentlyContinue
        Remove-Item Env:\GIT_PASSWORD -ErrorAction SilentlyContinue
        Remove-Item Env:\GIT_ASKPASS -ErrorAction SilentlyContinue
        Remove-Item Env:\GIT_TERMINAL_PROMPT -ErrorAction SilentlyContinue
        Remove-Item Env:\GCM_INTERACTIVE -ErrorAction SilentlyContinue
        Remove-Item -Path $askpass -Force -ErrorAction SilentlyContinue
    }
}

Set-Location $RepoRoot
Add-Result "Repository: $RepoRoot"
Add-Result "Branch: $(& $Git -C $RepoRoot branch --show-current)"

$jobs = @(
    @{
        Kind       = "github"
        Owner      = "Cubiczan"
        RemoteName = "github-cubiczan"
        RemoteUrl  = "https://github.com/Cubiczan/$RepoName.git"
        Prompt     = "GitHub PAT for Cubiczan (leave blank to skip)"
    },
    @{
        Kind       = "github"
        Owner      = "zan-maker"
        RemoteName = "github-zan-maker"
        RemoteUrl  = "https://github.com/zan-maker/$RepoName.git"
        Prompt     = "GitHub PAT for zan-maker (leave blank to skip)"
    },
    @{
        Kind       = "codeberg"
        Owner      = "Cubiczan"
        RemoteName = "codeberg"
        RemoteUrl  = "https://codeberg.org/Cubiczan/$RepoName.git"
        Prompt     = "Codeberg PAT for Cubiczan (leave blank to skip)"
    }
)

foreach ($job in $jobs) {
    Add-Result "`nPreparing $($job.RemoteName)"
    $token = Read-Pat $job.Prompt
    if (-not $token) {
        Add-Result "Skipped $($job.RemoteName)"
        continue
    }

    try {
        if ($job.Kind -eq "github") {
            Ensure-GitHubRepo -Owner $job.Owner -Token $token
        }
        else {
            Ensure-CodebergRepo -Owner $job.Owner -Token $token
        }
        Push-WithToken -RemoteName $job.RemoteName -RemoteUrl $job.RemoteUrl -Username $job.Owner -Token $token
    }
    catch {
        Add-Result "FAILED $($job.RemoteName): $($_.Exception.Message)"
    }
    finally {
        $token = $null
        [GC]::Collect()
    }
}

Add-Result "`nFinished: $(Get-Date -Format o)"
Add-Result "Results written to $ResultsPath"
Read-Host "Press Enter to close this window"
