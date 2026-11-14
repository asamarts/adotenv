import { createServer } from "node:http";
import { existsSync } from "node:fs";
import { mkdir, readFile, readdir } from "node:fs/promises";
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

async function newPage(browserInstance, viewport) {
    const current = await browserInstance.newPage({ viewport });
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

let base = "";

async function open(path) {
    await page.goto(base + path, { waitUntil: "networkidle" });
    await page.waitForSelector("#mdbook-content");
}

async function chartTable(kind) {
    return page.evaluate((kind) => {
        const tables = Array.from(document.querySelectorAll("#mdbook-content table"));
        const table = tables.find((candidate) => {
            const head = candidate.tHead && candidate.tHead.rows[0];
            if (!head) {
                return false;
            }
            const headers = Array.from(head.cells).map((cell) =>
                cell.textContent.replace(/[↕↑↓]/g, "").trim().toLowerCase(),
            );
            return kind === "memory"
                ? headers.includes("implementation") && headers.includes("peak heap") && headers.includes("retained heap")
                : headers.length > 1 && headers[0] === "benchmark" && headers[1] === "time";
        });
        if (!table) {
            return null;
        }
        return {
            headers: Array.from(table.tHead.rows[0].cells).map((cell) =>
                cell.textContent.replace(/[↕↑↓]/g, "").trim().toLowerCase(),
            ),
            rows: Array.from(table.tBodies[0].rows).map((row) =>
                Array.from(row.cells).map((cell) => cell.textContent.trim()),
            ),
        };
    }, kind);
}

function memoryRecords(table) {
    const at = (row, name) => row[table.headers.indexOf(name)];
    return table.rows.map((row) => ({
        size: at(row, "size"),
        shape: at(row, "shape"),
        mode: at(row, "mode"),
        implementation: at(row, "implementation"),
    }));
}

function speedRecords(table) {
    return table.rows
        .map((row) => row[0].split("/"))
        .filter((parts) => parts.length === 4 && parts[0].startsWith("competitor_"))
        .map((parts) => ({
            mode: parts[0].slice("competitor_".length),
            implementation: parts[1],
            shape: parts[2],
            size: parts[3],
        }));
}

async function currentChartKind() {
    return page.evaluate(() => {
        const tables = Array.from(document.querySelectorAll("#mdbook-content table"));
        for (const table of tables) {
            const head = table.tHead && table.tHead.rows[0];
            if (!head) {
                continue;
            }
            const headers = Array.from(head.cells).map((cell) =>
                cell.textContent.replace(/[↕↑↓]/g, "").trim().toLowerCase(),
            );
            if (headers.includes("implementation") && headers.includes("peak heap") && headers.includes("retained heap")) {
                return "memory";
            }
            if (headers.length > 1 && headers[0] === "benchmark" && headers[1] === "time") {
                return "speed";
            }
        }
        return "";
    });
}

async function discoverBenchmarkPages() {
    const directory = resolve(root, "benchmarks");
    const files = (await readdir(directory))
        .filter((name) => name.endsWith(".html"))
        .sort();
    return files.map((name) => `benchmarks/${name}`);
}

async function chartOnCurrent(path, kind) {
    check(await page.locator("#mdbook-content .bench-explorer").count() === 1, `${path}: chart is missing`);
    check(await page.locator("#mdbook-menu-bar a[rel=edit]").count() === 0, `${path}: edit-page icon is still present`);

    const table = await chartTable(kind);
    check(table !== null, `${path}: chart data table is missing`);
    if (table === null) {
        return;
    }
    const records = kind === "memory" ? memoryRecords(table) : speedRecords(table);
    const selectedMode = await page.locator(".bench-control").filter({ hasText: "Mode" }).locator("select").inputValue();
    const selectedShape = await page.locator(".bench-control").filter({ hasText: "Shape" }).locator("select").inputValue();
    const selected = records.filter((record) => record.mode === selectedMode && record.shape === selectedShape);
    const sizes = new Set(selected.map((record) => record.size));
    const implementations = new Set(selected.map((record) => record.implementation));
    const hasCompact = implementations.has("adotenv-compact") && implementations.has("adotenv");

    const controls = page.locator(".bench-control select");
    check(await page.locator(".bench-info-toggle").count() === await controls.count(), `${path}: selector help is incomplete`);
    const libraries = page.locator(".bench-control").filter({ hasText: "Libraries" }).locator("select");
    const viewOptions = await libraries.locator("option").allTextContents();
    check(viewOptions.includes("All libraries"), `${path}: all-libraries view is missing`);
    check(viewOptions.includes("Best competitor"), `${path}: best-competitor view is missing`);
    check(viewOptions.includes("Compact vs map") === hasCompact, `${path}: compact-vs-map option does not match the data`);

    const expectedAll = implementations.size * sizes.size;
    check(await page.locator(".bench-bar").count() === expectedAll, `${path}: all-libraries bar count is stale`);
    check(await page.locator(".bench-legend-item").count() === implementations.size, `${path}: all-libraries legend is stale`);

    if (hasCompact) {
        await libraries.selectOption({ label: "Compact vs map" });
        check(await page.locator(".bench-bar").count() === sizes.size * 2, `${path}: compact-vs-map bar count is stale`);
    }

    const bestSeries = hasCompact ? 3 : 2;
    await libraries.selectOption({ label: "Best competitor" });
    check(await page.locator(".bench-bar").count() === sizes.size * bestSeries, `${path}: best-competitor bar count is stale`);
    check(await page.locator(".bench-legend-item").count() === bestSeries, `${path}: best-competitor legend is stale`);

    await libraries.selectOption({ label: "All libraries" });
    const librariesControl = page.locator(".bench-control").filter({ hasText: "Libraries" });
    await librariesControl.locator(".bench-info-toggle").click();
    const popover = page.locator(".bench-info-popover:not([hidden])");
    check(await popover.count() === 1, `${path}: selector help did not open`);
    check((await popover.textContent()).includes("All libraries"), `${path}: selector help text is incomplete`);
    await page.keyboard.press("Escape");
    check(await page.locator(".bench-info-popover:not([hidden])").count() === 0, `${path}: Escape did not close selector help`);

    const tools = page.locator(".table-tools");
    check(await tools.count() === 1, `${path}: table tools are missing`);
    if (await tools.count() === 1) {
        const firstHeader = page.locator("#mdbook-content .sortable-header").first();
        await firstHeader.click();
        check((await firstHeader.getAttribute("aria-sort")) === "ascending", `${path}: table sorting did not activate`);
        const filter = page.locator(".table-filter");
        const totalRows = await page.locator("#mdbook-content tbody tr").count();
        await filter.fill("adotenv");
        const visibleRows = await page.locator("#mdbook-content tbody tr:visible").count();
        check(visibleRows > 0 && visibleRows < totalRows, `${path}: table filtering did not narrow the rows`);
    }
}

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
    check(await page.locator("#mdbook-sidebar .chapter ol.section").count() > 0, "nested sidebar is missing");
    check(await page.locator("#mdbook-menu-bar a[rel=edit]").count() === 0, "edit-page icon is still present");

    const benchmarkPages = await discoverBenchmarkPages();
    let chartPages = 0;
    let resultPages = 0;
    for (const path of benchmarkPages) {
        await open(path);
        const kind = await currentChartKind();
        if (kind) {
            chartPages += 1;
            await chartOnCurrent(path, kind);
        } else if (await page.locator(".bench-callout").count() > 0) {
            resultPages += 1;
            check(await page.locator("#mdbook-content .bench-explorer").count() === 0, `${path}: result page gained a chart`);
            check(await page.locator("#mdbook-menu-bar a[rel=edit]").count() === 0, `${path}: edit-page icon is still present`);
        }
    }
    check(chartPages >= 4, `discovered only ${chartPages} benchmark chart pages`);
    check(resultPages >= 2, `discovered only ${resultPages} result pages`);

    await page.setViewportSize({ width: 390, height: 844 });
    await open("benchmarks/compact-memory.html");
    const pageWidth = await page.evaluate(() => ({
        document: document.documentElement.scrollWidth,
        viewport: window.innerWidth,
    }));
    check(pageWidth.document <= pageWidth.viewport + 1, "mobile page overflows horizontally");
    check(await page.locator(".bench-scroll-hint").isVisible(), "mobile chart scroll hint is missing");
    await page.locator(".bench-info-toggle").last().click();
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
