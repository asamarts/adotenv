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

    tidyThemeMenu();
    addBrand();
    labelChapterLinks();
    addFooter();
})();
