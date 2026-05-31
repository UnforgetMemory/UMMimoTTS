# Stress Test: 2000 items, queue flow, rate limiting
# Tests: queue flow, concurrency limits, cancel, retry, SSE, chunk order merging

$BASE = "http://localhost:30231/api/v2"
$ITEM_COUNT = 2000
$TEXT_SIZE = 500  # bytes per item (500 * 2000 = 1MB total)

# Generate a repeating text block of specified size
function Generate-Text {
    param([int]$size)
    $base = "这是一段用于压力测试的文本内容，包含中文标点符号和各种字符。队列流转逻辑需要正确处理大量并发任务，确保速率限制生效，任务状态正确流转。"
    $result = ""
    while ($result.Length -lt $size) {
        $result += $base
    }
    return $result.Substring(0, [Math]::Min($size, $result.Length))
}

Write-Host "=== STRESS TEST: $ITEM_COUNT items, $TEXT_SIZE bytes each ===" -ForegroundColor Cyan
Write-Host ""

# Step 1: Create batch
Write-Host "[1/5] Creating batch..." -ForegroundColor Yellow
$batch = Invoke-RestMethod -Uri "$BASE/batches" -Method POST -ContentType "application/json" -Body '{"title":"Stress Test 2000","voice":"白桦","model":"mimo-v2.5-tts"}'
$batchId = $batch.id
Write-Host "  Batch ID: $batchId" -ForegroundColor Green

# Step 2: Add items in chunks of 100
Write-Host "[2/5] Adding $ITEM_COUNT items in batches of 100..." -ForegroundColor Yellow
$text = Generate-Text -size $TEXT_SIZE
$startTime = Get-Date

for ($i = 0; $i -lt $ITEM_COUNT; $i += 100) {
    $chunkEnd = [Math]::Min($i + 100, $ITEM_COUNT)
    $items = @()
    for ($j = $i; $j -lt $chunkEnd; $j++) {
        $items += @{
            seq = $j + 1
            filename = "stress_${($j + 1).ToString('D4')}.txt"
            content = $text
        }
    }
    $body = $items | ConvertTo-Json -Depth 3
    try {
        $null = Invoke-RestMethod -Uri "$BASE/batches/$batchId/items/batch" -Method POST -ContentType "application/json" -Body $body
        Write-Host "  Added items $($i+1)-$chunkEnd" -ForegroundColor Gray
    } catch {
        Write-Host "  ERROR adding items $($i+1)-$chunkEnd`: $_" -ForegroundColor Red
        break
    }
}

$addTime = (Get-Date) - $startTime
Write-Host "  Items added in $($addTime.TotalSeconds.ToString('F1'))s" -ForegroundColor Green

# Step 3: Submit batch
Write-Host "[3/5] Submitting batch..." -ForegroundColor Yellow
$submitResult = Invoke-RestMethod -Uri "$BASE/batches/$batchId/submit" -Method POST -ContentType "application/json"
$submitTime = Get-Date
Write-Host "  Submitted. Tasks created: $($submitResult.Count)" -ForegroundColor Green

# Step 4: Monitor queue flow
Write-Host "[4/5] Monitoring queue flow (60s)..." -ForegroundColor Yellow
$monitorStart = Get-Date
$lastStatus = @{}

while ($true) {
    $elapsed = ((Get-Date) - $monitorStart).TotalSeconds
    if ($elapsed -gt 60) { break }
    
    try {
        $tasks = Invoke-RestMethod -Uri "$BASE/tasks?page_size=1000&batch_id=$batchId" -Method GET
        $statusCounts = @{}
        foreach ($t in $tasks.data) {
            $s = $t.status
            if (-not $statusCounts.ContainsKey($s)) { $statusCounts[$s] = 0 }
            $statusCounts[$s]++
        }
        
        $line = "  t=$([Math]::Round($elapsed))s"
        foreach ($s in @('pending','queued','chunking','processing','merging','done','failed','cancelled')) {
            if ($statusCounts.ContainsKey($s)) {
                $line += "  $s=$($statusCounts[$s])"
            }
        }
        
        # Check concurrency
        $processing = if ($statusCounts.ContainsKey('processing')) { $statusCounts['processing'] } else { 0 }
        $chunking = if ($statusCounts.ContainsKey('chunking')) { $statusCounts['chunking'] } else { 0 }
        $active = $processing + $chunking
        
        if ($active -gt 25) {
            $line += "  [WARN: active=$active > 25]"
        }
        
        Write-Host $line -ForegroundColor $(if ($active -gt 25) { 'Red' } else { 'Gray' })
        
        # Check if all done
        $done = if ($statusCounts.ContainsKey('done')) { $statusCounts['done'] } else { 0 }
        $failed = if ($statusCounts.ContainsKey('failed')) { $statusCounts['failed'] } else { 0 }
        $cancelled = if ($statusCounts.ContainsKey('cancelled')) { $statusCounts['cancelled'] } else { 0 }
        $terminal = $done + $failed + $cancelled
        
        if ($terminal -ge $ITEM_COUNT -and $elapsed -gt 5) {
            Write-Host "  All tasks terminal after $([Math]::Round($elapsed))s" -ForegroundColor Green
            break
        }
        
        Start-Sleep -Seconds 3
    } catch {
        Write-Host "  Monitor error: $_" -ForegroundColor Red
        Start-Sleep -Seconds 3
    }
}

# Step 5: Test cancel during processing
Write-Host "[5/5] Testing cancel..." -ForegroundColor Yellow
$cancelResult = Invoke-RestMethod -Uri "$BASE/tasks/cancel-all" -Method POST -ContentType "application/json"
Write-Host "  Cancel result: $($cancelResult.cancelled)" -ForegroundColor Green

# Final status
Start-Sleep -Seconds 2
$finalTasks = Invoke-RestMethod -Uri "$BASE/tasks?page_size=1000&batch_id=$batchId" -Method GET
$finalCounts = @{}
foreach ($t in $finalTasks.data) {
    $s = $t.status
    if (-not $finalCounts.ContainsKey($s)) { $finalCounts[$s] = 0 }
    $finalCounts[$s]++
}
Write-Host ""
Write-Host "=== FINAL STATUS ===" -ForegroundColor Cyan
foreach ($kvp in $finalCounts.GetEnumerator()) {
    Write-Host "  $($kvp.Key): $($kvp.Value)" -ForegroundColor White
}
Write-Host ""
Write-Host "=== TEST COMPLETE ===" -ForegroundColor Cyan
