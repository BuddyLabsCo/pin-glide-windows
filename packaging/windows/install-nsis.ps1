$ErrorActionPreference = 'Stop'
choco install nsis --version=3.13 --yes --no-progress --allow-downgrade
if ($LASTEXITCODE -ne 0) { throw 'NSIS installation failed.' }
"${env:ProgramFiles(x86)}\NSIS" >> $env:GITHUB_PATH
