# Log File Processor

A Rust experiment in quickly parsing the specialist types of log files
we have at work (there is no work IP in this repo, it is all generic
code).

The aim was to go as fast as possible while keeping clear code.

A branch f-experiments contains various early variants, the master
branch has been pruned down to just my chosen one.

## Next Version

[Defaults.Matchers]
; Each one is a Regex. The names of the matchers define column names.
; What if some are position dependent?; Eg the log level
; The matches should be case-insensitive.

; I think we need the ability to split the message into (prelude, message, kvps)
; We will probably need a group so that we can extract the matched value.
; Special columns always defined: OriginalSource and Message
Timestamp = "^{\d}4 ..."
MachineName = "MachineName\w?=\w?
AppNamme = "..."
PID = "..."
TID = "..."
LogLevel = "..."
CorrelationKey = "...."
Source = ""
SysRef = "SysRef=[a-zA-Z0.9]{8}"
CallRecorderExecutionTime = "..."


; Specifies all the default settings.
[Defaults]
OutputColumns = "Timestamp, MachineName, AppName, SysRef ..."
Filters = []
From =  ; 1900-01-01
To =    ; 2099-01-01
SourceFiles = "*.log"
KibanaUrls = ""
SplunkUrls = ""




[UAT2Slowdown]
Filters = [
    "SysRef=some regex"
    ]     ; The first part is a field name, the second part a regex. Which can just be "QT123456" to match a literal.



Crates
======
rpassword (for reading)
https://github.com/hwchen/keyring-rs ?
Polyphase merge (need to write)
Persistent secure credentials
A case-insensitive string-keyed hash map.


