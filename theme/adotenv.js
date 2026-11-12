// adotenv documentation theme behavior. Loaded after mdBook's book.js.
(function () {
    "use strict";

    const html = document.documentElement;
    const root = typeof path_to_root === "string" ? path_to_root : "";

    // The theme menu offers Auto, Light and Dark. The other mdBook themes stay
    // valid for stored choices but are folded into these two looks.
    function tidyThemeMenu() {
        const menu = document.getElementById("mdbook-theme-list");
        if (!menu) {
            return;
        }
        let stored = null;
        try {
            stored = localStorage.getItem("mdbook-theme");
        } catch (e) { }
        const replacement = { rust: "light", coal: "navy", ayu: "navy" }[stored];
        if (replacement) {
            const button = document.getElementById("mdbook-theme-" + replacement);
            if (button) {
                button.click();
                const toggle = document.getElementById("mdbook-theme-toggle");
                if (menu.style.display === "block" && toggle) {
                    toggle.click();
                }
            }
        }
        ["rust", "coal", "ayu"].forEach(function (name) {
            const button = document.getElementById("mdbook-theme-" + name);
            if (button && button.parentElement) {
                button.parentElement.hidden = true;
            }
        });
        const dark = document.getElementById("mdbook-theme-navy");
        if (dark) {
            dark.textContent = "Dark";
        }
    }

    function addBrand() {
        const scrollbox = document.querySelector("#mdbook-sidebar .sidebar-scrollbox");
        if (!scrollbox || scrollbox.querySelector(".sidebar-brand")) {
            return;
        }
        const brand = document.createElement("a");
        brand.className = "sidebar-brand";
        brand.href = root + "index.html";
        brand.innerHTML =
            '<svg class="sidebar-brand-mark" viewBox="0 0 64 64" aria-hidden="true">' +
            '<rect width="64" height="64" rx="14" fill="currentColor"/>' +
            '<g fill="var(--bg)"><rect x="12" y="15" width="17" height="7" rx="3.5"/>' +
            '<rect x="12" y="28.5" width="12" height="7" rx="3.5"/>' +
            '<rect x="12" y="42" width="20" height="7" rx="3.5"/></g>' +
            '<g fill="var(--bg)" opacity="0.6"><rect x="33" y="15" width="19" height="7" rx="3.5"/>' +
            '<rect x="28" y="28.5" width="17" height="7" rx="3.5"/>' +
            '<rect x="36" y="42" width="10" height="7" rx="3.5"/></g></svg>' +
            '<span class="sidebar-brand-name">adotenv</span>';
        scrollbox.insertBefore(brand, scrollbox.firstChild);

        const api = document.createElement("a");
        api.className = "sidebar-api";
        api.href = "https://docs.rs/adotenv";
        api.innerHTML =
            '<span>API reference</span><span class="sidebar-api-host">docs.rs</span>' +
            '<svg viewBox="0 0 16 16" aria-hidden="true"><path d="M6 3h7v7M13 3 4 12" ' +
            'fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" ' +
            'stroke-linejoin="round"/></svg>';
        brand.insertAdjacentElement("afterend", api);
    }

    function chapterTitle(href) {
        const links = document.querySelectorAll("#mdbook-sidebar .chapter a[href]");
        for (const link of links) {
            if (link.href === href && !link.closest(".on-this-page")) {
                return link.textContent.trim();
            }
        }
        return null;
    }

    // Replace the arrow-only chapter links with labeled previous and next links.
    function labelChapterLinks() {
        const links = document.querySelectorAll(".nav-wrapper .mobile-nav-chapters");
        let labeled = 0;
        links.forEach(function (link) {
            const title = chapterTitle(link.href);
            if (!title) {
                return;
            }
            const label = document.createElement("span");
            label.className = "nav-label";
            label.textContent = link.classList.contains("previous") ? "Previous" : "Next";
            const name = document.createElement("span");
            name.className = "nav-title";
            name.textContent = title;
            link.append(label, name);
            labeled += 1;
        });
        if (labeled === links.length) {
            html.classList.add("adotenv-nav");
        }
    }

    function addFooter() {
        const content = document.getElementById("mdbook-content");
        if (!content || content.querySelector(".site-footer")) {
            return;
        }
        const footer = document.createElement("footer");
        footer.className = "site-footer";
        const items = [
            ["GitHub", "https://github.com/asamarts/adotenv"],
            ["crates.io", "https://crates.io/crates/adotenv"],
            ["API reference", "https://docs.rs/adotenv"],
            ["Changelog", "https://github.com/asamarts/adotenv/blob/main/CHANGELOG.md"],
        ];
        const edit = document.querySelector("#mdbook-menu-bar a[rel=edit]");
        if (edit) {
            items.push(["Edit this page", edit.href]);
        }
        items.forEach(function (item) {
            const link = document.createElement("a");
            link.textContent = item[0];
            link.href = item[1];
            footer.append(link);
        });
        const license = document.createElement("span");
        license.textContent = "MIT or Apache-2.0";
        footer.append(license);
        content.append(footer);
    }

    function numberFromText(text) {
        const cleaned = String(text).replace(/,/g, "").trim();
        const match = cleaned.match(/-?\d+(?:\.\d+)?/);
        return match ? Number(match[0]) : null;
    }

    function sizeRank(value) {
        const ranks = { "8": 8, "128": 128, "1k": 1000, "10k": 10000, "100k": 100000 };
        return ranks[value] || null;
    }

    function sortValue(text, header) {
        const trimmed = String(text).trim();
        if (header.toLowerCase() === "size") {
            const rank = sizeRank(trimmed);
            if (rank !== null) {
                return rank;
            }
        }
        const number = numberFromText(trimmed);
        return number === null ? trimmed.toLowerCase() : number;
    }

    function enhanceTables() {
        const content = document.getElementById("mdbook-content");
        if (!content) {
            return;
        }
        content.querySelectorAll("table").forEach(function (table) {
            if (table.dataset.adotenvEnhanced === "true") {
                return;
            }
            table.dataset.adotenvEnhanced = "true";
            table.classList.add("adotenv-table");
            const body = table.tBodies[0];
            const head = table.tHead && table.tHead.rows[0];
            if (!body || !head) {
                return;
            }
            const wrapper = table.closest(".table-wrapper") || table;
            const rows = Array.from(body.rows);
            const useFilter = rows.length >= 6;
            const tools = document.createElement("div");
            tools.className = "table-tools";
            const filter = document.createElement("input");
            filter.type = "search";
            filter.className = "table-filter";
            filter.placeholder = "Filter rows…";
            filter.setAttribute("aria-label", "Filter table rows");
            const count = document.createElement("span");
            count.className = "table-count";
            if (useFilter) {
                tools.append(filter, count);
                wrapper.parentNode.insertBefore(tools, wrapper);
            }
            rows.forEach(function (row, index) {
                row.dataset.originalOrder = String(index);
            });
            const headers = Array.from(head.cells);
            let sortIndex = -1;
            let sortDirection = 1;

            function updateCount() {
                const visible = rows.filter(function (row) { return !row.hidden; }).length;
                count.textContent = visible + " / " + rows.length + " rows";
            }

            function filterRows() {
                const query = filter.value.trim().toLowerCase();
                rows.forEach(function (row) {
                    row.hidden = query !== "" && !row.textContent.toLowerCase().includes(query);
                });
                updateCount();
            }

            function sortBy(index) {
                if (sortIndex === index) {
                    sortDirection = -sortDirection;
                } else {
                    sortIndex = index;
                    sortDirection = 1;
                }
                const header = headers[index].dataset.label || headers[index].textContent.trim();
                rows.sort(function (left, right) {
                    const a = sortValue(left.cells[index] ? left.cells[index].textContent : "", header);
                    const b = sortValue(right.cells[index] ? right.cells[index].textContent : "", header);
                    let result = 0;
                    if (typeof a === "number" && typeof b === "number") {
                        result = a - b;
                    } else {
                        result = String(a).localeCompare(String(b));
                    }
                    if (result === 0) {
                        return Number(left.dataset.originalOrder) - Number(right.dataset.originalOrder);
                    }
                    return result * sortDirection;
                });
                rows.forEach(function (row) { body.append(row); });
                headers.forEach(function (cell) {
                    cell.removeAttribute("aria-sort");
                    const marker = cell.querySelector(".sort-indicator");
                    if (marker) {
                        marker.textContent = "↕";
                    }
                });
                headers[index].setAttribute("aria-sort", sortDirection === 1 ? "ascending" : "descending");
                const marker = headers[index].querySelector(".sort-indicator");
                if (marker) {
                    marker.textContent = sortDirection === 1 ? "↑" : "↓";
                }
                filterRows();
            }

            headers.forEach(function (cell, index) {
                cell.dataset.label = cell.textContent.trim();
                cell.classList.add("sortable-header");
                cell.tabIndex = 0;
                cell.setAttribute("role", "button");
                cell.setAttribute("aria-label", "Sort by " + cell.dataset.label);
                const marker = document.createElement("span");
                marker.className = "sort-indicator";
                marker.textContent = "↕";
                marker.setAttribute("aria-hidden", "true");
                cell.append(marker);
                cell.addEventListener("click", function () { sortBy(index); });
                cell.addEventListener("keydown", function (event) {
                    if (event.key === "Enter" || event.key === " ") {
                        event.preventDefault();
                        sortBy(index);
                    }
                });
            });
            if (useFilter) {
                filter.addEventListener("input", filterRows);
                updateCount();
            }
        });
    }

    function formatBytes(value) {
        if (value >= 1024 * 1024) {
            return (value / (1024 * 1024)).toFixed(2) + " MiB";
        }
        if (value >= 1024) {
            return (value / 1024).toFixed(1) + " KiB";
        }
        return value + " B";
    }

    function formatCount(value) {
        return value.toLocaleString();
    }

    function formatNanoseconds(value) {
        if (value >= 1e6) {
            return (value / 1e6).toFixed(2) + " ms";
        }
        if (value >= 1e3) {
            return (value / 1e3).toFixed(1) + " µs";
        }
        return value.toFixed(0) + " ns";
    }

    function chartShell(parent, title, subtitle, controls) {
        const section = document.createElement("section");
        section.className = "bench-explorer";
        const heading = document.createElement("div");
        heading.className = "bench-explorer-heading";
        const h2 = document.createElement("h2");
        h2.textContent = title;
        const p = document.createElement("p");
        p.textContent = subtitle;
        heading.append(h2, p);
        const toolbar = document.createElement("div");
        toolbar.className = "bench-controls";
        controls.forEach(function (control) { toolbar.append(control); });
        const plot = document.createElement("div");
        plot.className = "bench-plot";
        const note = document.createElement("p");
        note.className = "bench-note";
        section.append(heading, toolbar, plot, note);
        parent.parentNode.insertBefore(section, parent);
        return { plot: plot, note: note };
    }

    function makeSelect(label, values, initial) {
        const wrapper = document.createElement("label");
        wrapper.className = "bench-control";
        const span = document.createElement("span");
        span.textContent = label;
        const select = document.createElement("select");
        values.forEach(function (value) {
            const option = document.createElement("option");
            option.value = value;
            option.textContent = value;
            if (value === initial) {
                option.selected = true;
            }
            select.append(option);
        });
        wrapper.append(span, select);
        return { wrapper: wrapper, select: select };
    }

    function createTableChart(table, kind) {
        const headers = Array.from(table.tHead.rows[0].cells).map(function (cell) {
            return cell.textContent.trim().toLowerCase();
        });
        const bodyRows = Array.from(table.tBodies[0].rows);
        const rows = [];
        const sizes = [];
        const shapes = [];
        const modes = [];
        if (kind === "memory") {
            const at = function (row, name) { return row.cells[headers.indexOf(name)]; };
            bodyRows.forEach(function (row) {
                const record = {
                    size: at(row, "size").textContent.trim(),
                    shape: at(row, "shape").textContent.trim(),
                    mode: at(row, "mode").textContent.trim(),
                    implementation: at(row, "implementation").textContent.trim(),
                    peak: numberFromText(at(row, "peak heap").textContent),
                    retained: numberFromText(at(row, "retained heap").textContent),
                    allocations: numberFromText(at(row, "allocations").textContent)
                };
                rows.push(record);
                if (!sizes.includes(record.size)) sizes.push(record.size);
                if (!shapes.includes(record.shape)) shapes.push(record.shape);
                if (!modes.includes(record.mode)) modes.push(record.mode);
            });
        } else {
            bodyRows.forEach(function (row) {
                const name = row.cells[0].textContent.trim();
                const match = name.match(/^competitor_(values|load)\/([^/]+)\/([^/]+)\/([^/]+)$/);
                if (!match) {
                    return;
                }
                const record = {
                    mode: match[1],
                    implementation: match[2],
                    shape: match[3],
                    size: match[4],
                    time: numberFromText(row.cells[1].textContent)
                };
                rows.push(record);
                if (!sizes.includes(record.size)) sizes.push(record.size);
                if (!shapes.includes(record.shape)) shapes.push(record.shape);
                if (!modes.includes(record.mode)) modes.push(record.mode);
            });
        }
        sizes.sort(function (a, b) { return (sizeRank(a) || 0) - (sizeRank(b) || 0); });
        const metric = kind === "memory" ? makeSelect("Metric", ["retained", "peak", "allocations"], "retained") : null;
        const mode = makeSelect("Mode", modes, modes.includes("values") ? "values" : modes[0]);
        const shape = makeSelect("Shape", shapes, shapes.includes("plain") ? "plain" : shapes[0]);
        const scale = makeSelect("Scale", ["log", "linear"], "log");
        const controls = [];
        if (metric) controls.push(metric.wrapper);
        controls.push(mode.wrapper, shape.wrapper, scale.wrapper);
        const shell = chartShell(
            table.closest(".table-wrapper") || table,
            kind === "memory" ? "Memory comparison" : "Speed comparison",
            kind === "memory"
                ? "Compare the compact API with the map API and the best pinned competitor. Lower is better."
                : "Compare compact parsing and loading with the map API and the best pinned competitor. Lower is better.",
            controls
        );

        function render() {
            const selectedMode = mode.select.value;
            const selectedShape = shape.select.value;
            const selectedMetric = kind === "memory" ? metric.select.value : "time";
            const selectedRows = rows.filter(function (row) {
                return row.mode === selectedMode && row.shape === selectedShape;
            });
            const chartRows = sizes.map(function (size) {
                const cell = selectedRows.filter(function (row) { return row.size === size; });
                const compactRow = cell.find(function (row) { return row.implementation === "adotenv-compact"; });
                const mapRow = cell.find(function (row) { return row.implementation === "adotenv"; });
                const competitors = cell.filter(function (row) {
                    return row.implementation !== "adotenv" && row.implementation !== "adotenv-compact";
                }).map(function (row) { return row[selectedMetric]; }).filter(function (value) { return value !== null; });
                return {
                    size: size,
                    compact: compactRow ? compactRow[selectedMetric] : null,
                    map: mapRow ? mapRow[selectedMetric] : null,
                    best: competitors.length ? Math.min.apply(null, competitors) : null
                };
            });
            const values = [];
            chartRows.forEach(function (row) {
                [row.compact, row.map, row.best].forEach(function (value) {
                    if (value !== null && !Number.isNaN(value)) values.push(value);
                });
            });
            const max = values.length ? Math.max.apply(null, values) : 0;
            if (!max) {
                shell.plot.textContent = "No non-zero data for this selection.";
                shell.note.textContent = "The table below has the exact zero-byte values.";
                return;
            }
            const width = 860;
            const height = 310;
            const left = 70;
            const right = 18;
            const top = 18;
            const bottom = 48;
            const plotWidth = width - left - right;
            const plotHeight = height - top - bottom;
            const groupWidth = plotWidth / sizes.length;
            const series = [
                { key: "compact", label: "Compact", color: "var(--accent)" },
                { key: "map", label: "Map", color: "var(--subtle)" },
                { key: "best", label: "Best competitor", color: "var(--chart-best)" }
            ].filter(function (item) {
                return chartRows.some(function (row) { return row[item.key] !== null; });
            });
            const barGap = 3;
            const barWidth = Math.max(4, (groupWidth * 0.78 - barGap * (series.length - 1)) / series.length);
            const useLog = scale.select.value === "log";
            const y = function (value) {
                const normalized = useLog ? Math.log10(value + 1) / Math.log10(max + 1) : value / max;
                return top + plotHeight - normalized * plotHeight;
            };
            const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
            svg.setAttribute("viewBox", "0 0 " + width + " " + height);
            svg.setAttribute("role", "img");
            svg.setAttribute("aria-label", (kind === "memory" ? "Memory" : "Speed") + " comparison chart");
            for (let tick = 0; tick <= 4; tick += 1) {
                const line = document.createElementNS("http://www.w3.org/2000/svg", "line");
                const lineY = top + (plotHeight * tick) / 4;
                line.setAttribute("x1", String(left));
                line.setAttribute("x2", String(width - right));
                line.setAttribute("y1", String(lineY));
                line.setAttribute("y2", String(lineY));
                line.setAttribute("class", "bench-grid-line");
                svg.append(line);
            }
            chartRows.forEach(function (row, groupIndex) {
                const groupStart = left + groupIndex * groupWidth + groupWidth * 0.11;
                series.forEach(function (item, seriesIndex) {
                    const value = row[item.key];
                    if (value === null) {
                        return;
                    }
                    const bar = document.createElementNS("http://www.w3.org/2000/svg", "rect");
                    const barHeight = top + plotHeight - y(value);
                    bar.setAttribute("x", String(groupStart + seriesIndex * (barWidth + barGap)));
                    bar.setAttribute("y", String(y(value)));
                    bar.setAttribute("width", String(barWidth));
                    bar.setAttribute("height", String(Math.max(1, barHeight)));
                    bar.setAttribute("rx", "2");
                    bar.setAttribute("class", "bench-bar bench-bar-" + item.key);
                    const title = document.createElementNS("http://www.w3.org/2000/svg", "title");
                    title.textContent = item.label + ": " + (kind === "memory"
                        ? (selectedMetric === "allocations" ? formatCount(value) + " allocations" : formatBytes(value))
                        : formatNanoseconds(value));
                    bar.append(title);
                    svg.append(bar);
                });
                const label = document.createElementNS("http://www.w3.org/2000/svg", "text");
                label.setAttribute("x", String(left + groupIndex * groupWidth + groupWidth / 2));
                label.setAttribute("y", String(height - 18));
                label.setAttribute("text-anchor", "middle");
                label.setAttribute("class", "bench-axis-label");
                label.textContent = row.size;
                svg.append(label);
            });
            const maxLabel = document.createElementNS("http://www.w3.org/2000/svg", "text");
            maxLabel.setAttribute("x", String(left - 8));
            maxLabel.setAttribute("y", String(top + 4));
            maxLabel.setAttribute("text-anchor", "end");
            maxLabel.setAttribute("class", "bench-axis-label");
            maxLabel.textContent = kind === "memory"
                ? (selectedMetric === "allocations" ? formatCount(max) : formatBytes(max))
                : formatNanoseconds(max);
            svg.append(maxLabel);
            shell.plot.replaceChildren(svg);
            const legend = document.createElement("div");
            legend.className = "bench-legend";
            series.forEach(function (item) {
                const span = document.createElement("span");
                span.className = "bench-legend-item";
                const swatch = document.createElement("i");
                swatch.className = "bench-swatch bench-bar-" + item.key;
                const label = document.createElement("span");
                label.textContent = item.label;
                span.append(swatch, label);
                legend.append(span);
            });
            shell.plot.append(legend);
            shell.note.textContent = (useLog ? "Logarithmic " : "") + "scale. Select " +
                (kind === "memory" ? "a metric, mode and shape" : "a mode and shape") +
                "; use the table below for exact values.";
        }

        if (metric) {
            metric.select.addEventListener("change", function () {
                render();
            });
        }
        [mode, shape, scale].filter(Boolean).forEach(function (control) {
            control.select.addEventListener("change", render);
        });
        render();
    }

    function enhanceCharts() {
        const content = document.getElementById("mdbook-content");
        if (!content) {
            return;
        }
        const tables = Array.from(content.querySelectorAll("table"));
        const headersOf = function (table) {
            const head = table.tHead && table.tHead.rows[0];
            return head ? Array.from(head.cells).map(function (cell) {
                return cell.textContent.trim().toLowerCase();
            }) : [];
        };
        const memory = tables.find(function (table) {
            const headers = headersOf(table);
            return headers.includes("implementation") && headers.includes("peak heap") && headers.includes("retained heap");
        });
        if (memory && !content.querySelector(".bench-explorer")) {
            createTableChart(memory, "memory");
        }
        const speed = tables.find(function (table) {
            const headers = headersOf(table);
            return headers.length > 1 && headers[0] === "benchmark" && headers[1] === "time";
        });
        if (speed && !content.querySelector(".bench-explorer")) {
            createTableChart(speed, "speed");
        }
    }

    tidyThemeMenu();
    addBrand();
    labelChapterLinks();
    addFooter();
    enhanceCharts();
    enhanceTables();
})();
