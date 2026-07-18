param(
  [Parameter(Mandatory = $true)]
  [string]$Expression,
  [int]$Port = 9222
)

$ErrorActionPreference = "Stop"
$targets = Invoke-RestMethod "http://127.0.0.1:$Port/json"
$target = @($targets) | Where-Object { $_.type -eq "page" -and $_.url -eq "http://tauri.localhost/" } | Select-Object -First 1
if (-not $target) { throw "OSL WebView target is unavailable" }

$socket = [System.Net.WebSockets.ClientWebSocket]::new()
try {
  [void]$socket.ConnectAsync([Uri]$target.webSocketDebuggerUrl, [Threading.CancellationToken]::None).GetAwaiter().GetResult()
  $message = @{
    id = 1
    method = "Runtime.evaluate"
    params = @{
      expression = $Expression
      awaitPromise = $true
      returnByValue = $true
      userGesture = $true
    }
  } | ConvertTo-Json -Compress -Depth 6
  $bytes = [Text.Encoding]::UTF8.GetBytes($message)
  [void]$socket.SendAsync(
    [ArraySegment[byte]]::new($bytes),
    [System.Net.WebSockets.WebSocketMessageType]::Text,
    $true,
    [Threading.CancellationToken]::None
  ).GetAwaiter().GetResult()

  $buffer = [byte[]]::new(65536)
  $stream = [IO.MemoryStream]::new()
  do {
    $result = $socket.ReceiveAsync(
      [ArraySegment[byte]]::new($buffer),
      [Threading.CancellationToken]::None
    ).GetAwaiter().GetResult()
    $stream.Write($buffer, 0, $result.Count)
  } until ($result.EndOfMessage)
  [Text.Encoding]::UTF8.GetString($stream.ToArray())
} finally {
  if ($socket.State -eq [System.Net.WebSockets.WebSocketState]::Open) {
    [void]$socket.CloseAsync(
      [System.Net.WebSockets.WebSocketCloseStatus]::NormalClosure,
      "done",
      [Threading.CancellationToken]::None
    ).GetAwaiter().GetResult()
  }
  $socket.Dispose()
}
