import { createServer } from "node:http";
import { existsSync } from "node:fs";
import { mkdir, readFile } from "node:fs/promises";
import { dirname, extname, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { chromium } from "playwright-core";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "../..");
const root = resolve(process.env.DOCS_ROOT || resolve(repoRoot, "target/book"));
const screenshotDir = resolve(repoRoot, "target/docs-smoke");
const failures = [];
let page;
let browser;

function check(condition, message) {
    if (!condition) {
        failures.push(message);
    }
}

function mimeType(path) {
    return {
        ".html": "text/html; charset=utf-8",
        ".css": "text/css; charset=utf-8",
        ".js": "text/javascript; charset=utf-8",
        ".json": "application/json; charset=utf-8",
        ".svg": "image/svg+xml",
        ".png": "image/png",
        ".woff": "font/woff",
        ".woff2": "font/woff2",
    }[extname(path)] || "application/octet-stream";
}

function chromePath() {
    const candidates = [
        process.env.CHROME_PATH,
        "/usr/bin/google-chrome",
        "/usr/bin/google-chrome-stable",
        "/usr/bin/chromium",
        "/usr/bin/chromium-browser",
    ].filter(Boolean);
    const found = candidates.find((candidate) => existsSync(candidate));
    if (!found) {
        throw new Error(
            "No Chrome executable found. Set CHROME_PATH to a Chrome/Chromium binary.",
        );
    }
    return found;
}

async function newPage(browser, viewport) {
    const current = await browser.newPage({ viewport });
    current.on("pageerror", (error) => failures.push(`page error: ${error.message}`));
    current.on("console", (message) => {
        if (message.type() === "error") {
            failures.push(`console error: ${message.text()}`);
        }
    });
    return current;
}

async function shot(name) {
    if (!page) {
        return;
    }
    await mkdir(screenshotDir, { recursive: true });
    await page.screenshot({ path: resolve(screenshotDir, name + ".png") });
}

async function open(path) {
    await page.goto(base + path, { waitUntil: "networkidle" });
    await page.waitForSelector("#mdbook-content");
}

let base = "";
const server = createServer(async (request, response) => {
    try {
        const pathname = decodeURIComponent(new URL(request.url, "http://localhost").pathname);
        const relative = pathname === "/" ? "index.html" : pathname.slice(1);
        const file = resolve(root, relative);
        if (file !== root && !file.startsWith(root + sep)) {
            response.writeHead(403);
            response.end();
            return;
        }
        const data = await readFile(file);
        response.writeHead(200, { "content-type": mimeType(file) });
        response.end(data);
    } catch {
        response.writeHead(404);
        response.end();
    }
});

try {
    await new Promise((resolveServer) => server.listen(0, "127.0.0.1", resolveServer));
    const port = server.address().port;
    base = `http://127.0.0.1:${port}/`;

    browser = await chromium.launch({
        headless: true,
        executablePath: chromePath(),
        args: ["--no-sandbox", "--disable-dev-shm-usage"],
    });
    page = await newPage(browser, { width: 1440, height: 1000 });

    await open("benchmarks/compact-memory.html");
    check(await page.locator("#mdbook-content .bench-explorer").count() === 1, "memory chart is missing");
    check(await page.locator("#mdbook-content .table-tools").count() === 1, "memory table filter is missing");
    check(await page.locator(".bench-info-toggle").count() === 5, "memory selector help is incomplete");
    check(await page.locator("#mdbook-menu-bar a[rel=edit]").count() === 0, "edit-page icon is still present");
    check(await page.locator("#mdbook-sidebar .chapter ol.section").count() > 0, "nested sidebar is missing");

    const memoryBars = await page.locator(".bench-bar").count();
    check(memoryBars === 25, `expected 25 default memory bars, found ${memoryBars}`);
    const memoryLegend = await page.locator(".bench-legend-item").count();
    check(memoryLegend === 5, `expected 5 default memory series, found ${memoryLegend}`);

    const libraries = page.locator(".bench-control").filter({ hasText: "Libraries" }).locator("select");
    await libraries.selectOption({ label: "Compact vs map" });
    check(await page.locator(".bench-bar").count() === 10, "compact-vs-map view did not redraw");
    await libraries.selectOption({ label: "Best competitor" });
    check(await page.locator(".bench-bar").count() === 15, "best-competitor view did not redraw");
    await libraries.selectOption({ label: "All libraries" });
    check(await page.locator(".bench-bar").count() === 25, "all-libraries view did not redraw");

    await page.locator(".bench-info-toggle").nth(1).click();
    const openPopover = page.locator(".bench-info-popover:not([hidden])");
    check(await openPopover.count() === 1, "selector help did not open");
    check((await openPopover.textContent()).includes("All libraries"), "libraries help text is missing");
    await page.keyboard.press("Escape");
    check(await page.locator(".bench-info-popover:not([hidden])").count() === 0, "Escape did not close selector help");

    const firstHeader = page.locator("#mdbook-content .sortable-header").first();
    await firstHeader.click();
    check((await firstHeader.getAttribute("aria-sort")) === "ascending", "table sorting did not activate");
    const filter = page.locator(".table-filter");
    await filter.fill("adotenv-compact");
    const visibleRows = await page.locator("#mdbook-content tbody tr:visible").count();
    check(visibleRows > 0 && visibleRows < 165, "table filtering did not narrow the rows");

    await open("benchmarks/speed-1.5.0.html");
    check(await page.locator("#mdbook-content .bench-explorer").count() === 1, "speed chart is missing");
    check(await page.locator(".bench-info-toggle").count() === 4, "speed selector help is incomplete");
    const speedBars = await page.locator(".bench-bar").count();
    check(speedBars >= 25, `expected the all-libraries speed view, found ${speedBars} bars`);
    await page.locator(".bench-control").filter({ hasText: "Mode" }).locator("select").selectOption("load");
    check(await page.locator(".bench-bar").count() === 30, "load chart did not include every loader");
    check((await page.locator(".bench-legend-item").allTextContents()).includes("dotenv-flow"), "loader legend is incomplete");

    await open("benchmarks/RESULTS-1.5.0.html");
    check(await page.locator(".bench-callout").count() === 2, "result chart links are missing");
    check(await page.locator("#mdbook-content .bench-explorer").count() === 0, "result page gained a chart");
    check(await page.locator("#mdbook-menu-bar a[rel=edit]").count() === 0, "edit-page icon is still present on results");

    await page.setViewportSize({ width: 390, height: 844 });
    await open("benchmarks/compact-memory.html");
    const pageWidth = await page.evaluate(() => ({
        document: document.documentElement.scrollWidth,
        viewport: window.innerWidth,
    }));
    check(pageWidth.document <= pageWidth.viewport + 1, "mobile page overflows horizontally");
    await page.locator(".bench-info-toggle").nth(4).click();
    const mobilePopover = await page.locator(".bench-info-popover:not([hidden])").boundingBox();
    check(
        mobilePopover !== null &&
            mobilePopover.x >= 0 &&
            mobilePopover.y >= 0 &&
            mobilePopover.x + mobilePopover.width <= 390 &&
            mobilePopover.y + mobilePopover.height <= 844,
        "mobile selector help overflows the viewport",
    );

    if (failures.length > 0) {
        await shot("failure");
        throw new Error(failures.join("\n"));
    }
    console.log("docs smoke: PASS");
} catch (error) {
    await shot("failure");
    throw error;
} finally {
    if (browser) {
        await browser.close().catch(() => {});
    }
    await new Promise((resolveServer) => server.close(() => resolveServer()));
}
