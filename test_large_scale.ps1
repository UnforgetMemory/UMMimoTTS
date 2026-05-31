# Large Scale Queue Flow Test
# Tests rate limiting and queue flow with 2000 items

param(
    [int]$ItemCount = 2000,
    [int]$MonitorSeconds = 120
)

$BASE_URL = "http://localhost:30231"

# Helper function for API calls
function Invoke-Api {
    param([string]$Method, [string]$Path, $Body)
    $params = @{
        Uri = "$BASE_URL$Path"
        Method = $Method
        ContentType = "application/json"
    }
    if ($Body) {
        $params.Body = $Body | ConvertTo-Json -Depth 10
    }
    try {
        $response = Invoke-RestMethod @params
        return $response
    } catch {
        Write-Host "ERROR: $_" -ForegroundColor Red
        return $null
    }
}

# Create batch
Write-Host "`n=== CREATING BATCH WITH $ItemCount ITEMS ===" -ForegroundColor Cyan
$batch = Invoke-Api -Method "POST" -Path "/api/v2/batches" -Body @{
    title = "Large Scale Test - $ItemCount items"
    voice = "白桦"
    model = "mimo-v2.5-tts"
    speed = 1.0
}
if (-not $batch) { Write-Host "Failed to create batch" -ForegroundColor Red; exit 1 }
$batchId = $batch.id
Write-Host "Batch created: $batchId" -ForegroundColor Green

# Add items
Write-Host "`n=== ADDING ITEMS ===" -ForegroundColor Cyan
$items = @()
for ($i = 1; $i -le $ItemCount; $i++) {
    $items += @{
        seq = $i
        filename = "item_$($i.ToString('D4')).txt"
        content = "这是第 $i 个测试文本，用于测试大批量队列处理和速率限制功能。The quick brown fox jumps over the lazy dog. 这个句子包含了英文字母和中文字符。"
    }
}

$addResult = Invoke-Api -Method "POST" -Path "/api/v2/batches/$batchId/items/batch" -Body $items
if ($addResult) {
    Write-Host "Added $($items.Count) items" -ForegroundColor Green
} else {
    Write-Host "Failed to add items" -ForegroundColor Red; exit 1
}

# Submit batch
Write-Host "`n=== SUBMITTING BATCH ===" -ForegroundColor Cyan
$submitResult = Invoke-Api -Method "POST" -Path "/api/v2/batches/$batchId/submit"
if ($submitResult) {
    Write-Host "Batch submitted" -ForegroundColor Green
} else {
    Write-Host "Failed to submit batch" -ForegroundColor Red; exit 1
}

# Monitor queue flow
Write-Host "`n=== MONITORING QUEUE FLOW FOR $MonitorSeconds SECONDS ===" -ForegroundColor Cyan
Write-Host "Polling every 3 seconds...`n" -ForegroundColor Gray

$startTime = Get-Date
$endTime = $startTime.AddSeconds($MonitorSeconds)
$pollCount = 0

while ((Get-Date) -lt $endTime) {
    Start-Sleep -Seconds 3
    $pollCount++
    
    # Get task statistics
    $tasks = Invoke-Api -Method "GET" -Path "/api/v2/tasks?page_size=1"
    if (-not $tasks) { continue }
    
    # Count by status
    $statusCounts = @{}
    $allTasks = @()
    $page = 0
    $pageSize = 100
    
    do {
        $taskPage = Invoke-Api -Method "GET" -Path "/api/v2/tasks?page=$page&per_page=$pageSize"
        if ($taskPage -and $taskPage.data) {
            $allTasks += $taskPage.data
            $page++
        } else {
            break
        }
    } while ($taskPage.data.Count -eq $pageSize)
    
    foreach ($task in $allTasks) {
        $status = $task.status
        if (-not $statusCounts.ContainsKey($status)) {
            $statusCounts[$status] = 0
        }
        $statusCounts[$status]++
    }
    
    # Display status
    $elapsed = [math]::Round(((Get-Date) - $startTime).TotalSeconds)
    $statusLine = "[$elapsed s] "
    
    $statusOrder = @("pending", "queued", "chunking", "processing", "merging", "done", "failed", "cancelled")
    foreach ($status in $statusOrder) {
        if ($statusCounts.ContainsKey($status)) {
            $count = $statusCounts[$status]
            $color = switch ($status) {
                "pending"   { "Gray" }
                "queued"    { "Yellow" }
                "chunking"  { "DarkYellow" }
                "processing"{ "Cyan" }
                "merging"   { "Blue" }
                "done"      { "Green" }
                "failed"    { "Red" }
                "cancelled" { "DarkGray" }
                default     { "White" }
            }
            Write-Host "$status`: $count " -ForegroundColor $color -NoNewline
        }
    }
    Write-Host ""
    
    # Check if all done
    $terminalCount = ($statusCounts["done"] ?? 0) + ($statusCounts["failed"] ?? 0) + ($statusCounts["cancelled"] ?? 0)
    if ($terminalCount -ge $ItemCount) {
        Write-Host "`n=== ALL TASKS REACHED TERMINAL STATE ===" -ForegroundColor Green
        break
    }
}

# Final summary
Write-Host "`n=== FINAL SUMMARY ===" -ForegroundColor Cyan
Write-Host "Batch ID: $batchId" -ForegroundColor White
Write-Host "Total items: $ItemCount" -ForegroundColor White
Write-Host "Monitoring duration: $([math]::Round(((Get-Date) - $startTime).TotalSeconds)) seconds" -ForegroundColor White
Write-Host "Polls performed: $pollCount" -ForegroundColor White

# Final status counts
Write-Host "`nFinal Status Counts:" -ForegroundColor White
foreach ($status in $statusCounts.Keys | Sort-Object) {
    Write-Host "  $status`: $($statusCounts[$status])" -ForegroundColor $(switch ($status) {
        "done"      { "Green" }
        "failed"    { "Red" }
        "cancelled" { "DarkGray" }
        "processing"{ "Cyan" }
        default     { "White" }
    })
}

# Rate limiting analysis
$processingTasks = $allTasks | Where-Object { $_.status -eq "processing" }
if ($processingTasks.Count -gt 20) {
    Write-Host "`nWARNING: More than 20 tasks in 'processing' state!" -ForegroundColor Red
    Write-Host "Task-level concurrency gate may not be working." -ForegroundColor Red
} else {
    Write-Host "`nTask-level concurrency working: max $($processingTasks.Count) tasks in processing" -ForegroundColor Green
}
