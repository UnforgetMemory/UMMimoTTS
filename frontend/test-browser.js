const { chromium } = require('playwright-core');
(async () => {
    try {
        const browser = await chromium.connectOverCDP('http://localhost:9222');
        const contexts = browser.contexts();
        console.log('Contexts:', contexts.length);
        
        for (const context of contexts) {
            const pages = context.pages();
            console.log('Pages:', pages.length);
            for (const page of pages) {
                console.log('URL:', page.url());
                console.log('Title:', await page.title());
                await page.screenshot({ path: 'C:\\Temp\\mimo-tts-screenshot.png', fullPage: true });
                console.log('Screenshot saved');
            }
        }
        await browser.close();
    } catch (e) {
        console.error('Error:', e.message);
    }
})();
