import { chromium } from 'playwright-core';
const browser = await chromium.connectOverCDP('http://localhost:9222');
const contexts = browser.contexts();
for (const context of contexts) {
    const pages = context.pages();
    for (const page of pages) {
        console.log('URL:', page.url());
        
        // Find and click the sidebar toggle button (合成任务 or similar)
        const buttons = await page.locator('button').all();
        console.log('Buttons found:', buttons.length);
        for (const btn of buttons) {
            const text = await btn.textContent();
            const visible = await btn.isVisible();
            console.log(`  "${text.trim()}" visible:${visible}`);
        }
        
        // Look for FloatingToolbar - it has the toggle button
        const toolbar = await page.locator('[class*="floating"], [class*="toolbar"]').first();
        if (toolbar) {
            console.log('Found toolbar');
            const toolbarText = await toolbar.textContent();
            console.log('Toolbar content:', toolbarText.trim().substring(0, 200));
        }
        
        // Look for 合成任务 button
        const batchBtn = await page.locator('button:has-text("合成任务")').first();
        if (batchBtn) {
            const visible = await batchBtn.isVisible();
            console.log('合成任务 button visible:', visible);
            if (visible) {
                await batchBtn.click();
                await page.waitForTimeout(1000);
                console.log('Clicked 合成任务 button');
                
                // Check if sidebar appeared
                const sidebar = await page.locator('aside').first();
                if (sidebar) {
                    const sidebarVisible = await sidebar.isVisible();
                    console.log('Sidebar visible after click:', sidebarVisible);
                    const sidebarText = await sidebar.textContent();
                    console.log('Sidebar content:', sidebarText?.trim().substring(0, 300));
                }
            }
        }
        
        // Take screenshot after clicking
        await page.screenshot({ path: 'C:\\Temp\\mimo-tts-sidebar.png', fullPage: true });
        console.log('Screenshot saved');
    }
}
await browser.close();
