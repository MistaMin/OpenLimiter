; Windows installer for OpenLimiter: VST3 and CLAP, one choice per format.
; Built by .github/workflows/build.yml:  iscc /DVersion=1.0.0-Beta /DDist=<dist> /DRoot=<repo> installer\windows.iss
#ifndef Version
  #define Version "0.0.0"
#endif
#ifndef Dist
  #define Dist "..\dist-windows"
#endif
#ifndef Root
  #define Root ".."
#endif

[Setup]
AppId={{6F1D5E2A-3B7C-4C59-9E21-0A4C7B8D1E53}
AppName=OpenLimiter
AppVersion={#Version}
AppPublisher=OpenGrid
AppPublisherURL=https://opengrid.studio
DefaultDirName={commonpf64}\OpenGrid\OpenLimiter
DisableDirPage=yes
DisableProgramGroupPage=yes
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
PrivilegesRequired=admin
LicenseFile={#Root}\BINARY_LICENSE.txt
OutputBaseFilename=OpenLimiter-{#Version}-windows-x64-setup
OutputDir={#Dist}\..
Compression=lzma2
SolidCompression=yes
UninstallDisplayName=OpenLimiter {#Version}

[Types]
Name: "full"; Description: "VST3 and CLAP"
Name: "custom"; Description: "Choose formats"; Flags: iscustom

[Components]
Name: "vst3"; Description: "VST3 plug-in"; Types: full
Name: "clap"; Description: "CLAP plug-in"; Types: full

[Files]
Source: "{#Dist}\OpenLimiter.vst3\*"; DestDir: "{commoncf64}\VST3\OpenLimiter.vst3"; Components: vst3; Flags: recursesubdirs ignoreversion
Source: "{#Dist}\OpenLimiter.clap"; DestDir: "{commoncf64}\CLAP"; Components: clap; Flags: ignoreversion
Source: "{#Root}\LICENSE"; DestDir: "{app}\Licenses"; DestName: "LICENSE.txt"
Source: "{#Root}\BINARY_LICENSE.txt"; DestDir: "{app}\Licenses"
Source: "{#Root}\THIRD_PARTY_LICENSES.md"; DestDir: "{app}\Licenses"
Source: "{#Root}\Licenses\*"; DestDir: "{app}\Licenses"
