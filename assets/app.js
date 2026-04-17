const view = document.getElementById("view");

function formatSize(bytes) {
  if (bytes == null) return "—";
  const units = ["B", "KB", "MB", "GB", "TB", "PB"];
  let n = Number(bytes);
  let i = 0;
  while (n >= 1024 && i < units.length - 1) { n /= 1024; i++; }
  return (i === 0 ? n.toString() : n.toFixed(2)) + " " + units[i];
}

function formatTime(ts) {
  if (!ts) return "—";
  return new Date(ts * 1000).toISOString().replace("T", " ").slice(0, 16);
}

function basename(path) {
  if (!path) return "";
  const clean = path.replace(/\/$/, "");
  const i = clean.lastIndexOf("/");
  return i < 0 ? clean : clean.slice(i + 1);
}

// A clickable tag chip. Navigates to the search view pre-filtered on this tag.
function tagChip(t) {
  const label = t.value ? `${t.key}=${t.value}` : t.key;
  const query = t.value ? `${t.key}=${t.value}` : t.key;
  return el(
    "a",
    {
      class: "tag tag-link",
      href: `#/search?tag=${encodeURIComponent(query)}`,
      title: `Show all with tag ${label}`,
    },
    label,
  );
}

const TYPE_ICONS = {
  "ai-model": "🤖",
  image: "🖼",
  audio: "🎵",
  video: "🎬",
  game: "🎮",
  application: "📦",
  document: "📄",
  book: "📚",
  code: "📜",
  archive: "🗜",
  cache: "🧹",
  home: "🏠",
  system: "🔧",
  config: "🛠",
  boot: "🐧",
  devices: "🔌",
  swap: "🔄",
  services: "🛎",
  procfs: "🧠",
  sysfs: "🧬",
  mount: "💾",
  gamedata: "💽",
  emulator: "🕹",
  dependencies: "📦",
  "build-artifact": "🏗",
  inbox: "📥",
  generic: "📁",
};

function typeEmoji(baseType) {
  return TYPE_ICONS[baseType] || "";
}

// Derive a human label from a collection's base_type and tags, in the form
// "<Type> · <scope>". Type comes from base_type; scope comes from the most
// distinctive tag (library, artist=X, album=X, store=X, ...).
function kindLabel(c) {
  const tags = c.tags || [];
  const byKey = Object.fromEntries(tags.map(t => [t.key, t.value]));

  const type = typeLabel(c.base_type, byKey);
  const scope = scopeLabel(byKey);
  return scope ? `${type} · ${scope}` : type;
}

function typeLabel(baseType, byKey) {
  if (baseType && baseType !== "generic") return capitalize(baseType);
  if (byKey.system !== undefined) return "System";
  return "Folder";
}

function scopeLabel(byKey) {
  if (byKey.library !== undefined) return "library";
  if (byKey.store) return `store=${byKey.store}`;
  if (byKey.platform) return `platform=${byKey.platform}`;
  if (byKey.vendor) return `vendor=${byKey.vendor}`;
  if (byKey.artist) return `artist=${byKey.artist}`;
  if (byKey.album) return `album=${byKey.album}`;
  if (byKey.series) return `series=${byKey.series}`;
  if (byKey.contains) return byKey.contains;
  if (byKey.mounts) return `${byKey.mounts} mounts`;
  if (byKey.emulator) return `emulator=${byKey.emulator}`;
  if (byKey.runtime) return `runtime=${byKey.runtime}`;
  if (byKey.kind) return byKey.kind;
  return null;
}

function capitalize(s) {
  return s ? s.charAt(0).toUpperCase() + s.slice(1) : s;
}

function relativeSegment(childPath, parentPath) {
  if (!parentPath) return childPath;
  const prefix = parentPath.replace(/\/$/, "") + "/";
  return childPath.startsWith(prefix) ? childPath.slice(prefix.length) : childPath;
}

function el(tag, attrs = {}, ...children) {
  const e = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") e.className = v;
    else if (k.startsWith("on")) e.addEventListener(k.slice(2), v);
    else if (v === true) e.setAttribute(k, "");
    else if (v === false || v == null) {}
    else e.setAttribute(k, v);
  }
  for (const c of children) {
    if (c == null) continue;
    e.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
  }
  return e;
}

// Parse a formatted size string (e.g. "4.2 MB") back to bytes for sorting.
// Returns null when the string isn't a size — caller should treat as "no key".
const SIZE_UNITS = { B: 1, KB: 1024, MB: 1024 ** 2, GB: 1024 ** 3, TB: 1024 ** 4, PB: 1024 ** 5 };
function parseSize(s) {
  const m = /^([\d.]+)\s*(B|KB|MB|GB|TB|PB)$/i.exec(s?.trim() || "");
  if (!m) return null;
  return parseFloat(m[1]) * SIZE_UNITS[m[2].toUpperCase()];
}

// Make a table's header cells clickable to sort the body.
// `getters` is an array aligned with columns; each entry is either:
//   - a function(tr) -> sortable value (string/number/null), or
//   - null/undefined to leave the column unsortable.
// Clicking a sorted column toggles direction. Only real data rows are
// sorted; helper rows (e.g. .classify-form-row) stay anchored to wherever
// the DOM puts them — good enough in practice.
function makeSortable(table, getters) {
  const ths = table.querySelectorAll("thead th");
  const tbody = table.querySelector("tbody");
  if (!tbody) return;
  let currentCol = null;
  let asc = true;

  ths.forEach((th, idx) => {
    const getter = getters[idx];
    if (!getter) return;
    th.classList.add("sortable");
    th.addEventListener("click", () => {
      if (currentCol === idx) asc = !asc;
      else { currentCol = idx; asc = true; }

      const rows = Array.from(tbody.children).filter(r => r.tagName === "TR");
      rows.sort((a, b) => {
        const av = getter(a);
        const bv = getter(b);
        if (av === bv) return 0;
        if (av == null) return 1;
        if (bv == null) return -1;
        const diff = (typeof av === "number" && typeof bv === "number")
          ? av - bv
          : String(av).localeCompare(String(bv), undefined, { numeric: true, sensitivity: "base" });
        return asc ? diff : -diff;
      });
      for (const r of rows) tbody.appendChild(r);

      ths.forEach(t => t.classList.remove("sort-asc", "sort-desc"));
      th.classList.add(asc ? "sort-asc" : "sort-desc");
    });
  });
}

function mount(templateId) {
  const tpl = document.getElementById(templateId);
  view.replaceChildren(tpl.content.cloneNode(true));
}

async function fetchJson(path) {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${path}: ${res.status}`);
  return res.json();
}

function browseHref(path) {
  return path ? `#/browse?path=${encodeURIComponent(path)}` : "#/";
}

// ---------- Browse (path-based) ----------

async function showBrowse(params) {
  mount("tpl-browse");
  const path = params.get("path") || "/";
  const url = `/api/browse?path=${encodeURIComponent(path)}`;
  const data = await fetchJson(url);

  // Breadcrumbs: show every path segment, whether indexed or not.
  // Separators are literal "/" so selecting across the breadcrumb yields
  // a copyable absolute path.
  const crumbs = view.querySelector("#breadcrumbs");
  crumbs.appendChild(el("a", { href: browseHref("/") }, "/"));
  const parts = (data.path || "/").split("/").filter(Boolean);
  let acc = "";
  for (const [i, part] of parts.entries()) {
    acc += "/" + part;
    if (i > 0) crumbs.appendChild(el("span", { class: "sep" }, "/"));
    crumbs.appendChild(el("a", { href: browseHref(acc) }, part));
  }

  // Current collection metadata rendered inline next to the breadcrumb.
  if (data.current) {
    const cur = data.current;
    const metaBox = view.querySelector("#current");
    metaBox.hidden = false;
    const baseTypeField = metaBox.querySelector('[data-field="base_type"]');
    const baseLink = el("a", {
      class: "kind-pill kind-indexed kind-link",
      href: `#/search?type=${encodeURIComponent(cur.base_type)}`,
      title: `Show all ${cur.base_type}`,
    }, cur.base_type);
    baseTypeField.replaceWith(baseLink);

    const privacyField = metaBox.querySelector('[data-field="privacy"]');
    const priv = cur.privacy || "public";
    const privacyGlyphs = { public: "", personal: "👤", confidential: "🔒" };
    privacyField.textContent = privacyGlyphs[priv] || "";
    privacyField.title = `privacy: ${priv}`;
    if (!privacyGlyphs[priv] || priv === "public") privacyField.hidden = true;

    const tagsDiv = metaBox.querySelector('[data-field="tags"]');
    for (const t of cur.tags || []) {
      tagsDiv.appendChild(tagChip(t));
    }
  }

  const entriesBody = view.querySelector("#entries-body");
  view.querySelector("#entries-count").textContent = `${data.entries.length}`;

  if (data.entries.length === 0) {
    const section = view.querySelector(".browse");
    section.appendChild(el("p", { class: "loading" },
      "This directory is empty or not readable."));
    return;
  }

  const unknownCount = data.entries.filter(e => e.state === "unknown").length;
  renderBulkClassifyBar(data.path || path, unknownCount);

  for (const e of data.entries) {
    for (const row of renderEntryRows(e)) entriesBody.appendChild(row);
  }

  const entriesTable = entriesBody.closest("table");
  if (entriesTable) {
    // Columns: [icon, Name, Kind, Info, actions]
    makeSortable(entriesTable, [
      null,
      tr => tr.querySelector("td.name")?.textContent?.trim() ?? "",
      tr => tr.children[2]?.textContent?.trim() ?? "",
      tr => {
        const raw = tr.children[3]?.textContent?.trim() ?? "";
        const size = parseSize(raw);
        return size != null ? size : raw;
      },
      null,
    ]);
  }

  wireScanBar(data.path || path);
}

function renderBulkClassifyBar(currentPath, unknownCount) {
  const section = view.querySelector(".entries-section");
  if (!section || unknownCount < 2) return;
  const bar = el("div", { class: "bulk-bar" });
  const btn = el("button", {
    type: "button",
    class: "bulk-btn",
  }, `Classify all ${unknownCount} unknowns…`);
  btn.addEventListener("click", () => toggleBulkForm(bar, currentPath));
  bar.appendChild(btn);
  section.insertBefore(bar, section.firstChild.nextSibling);
}

function toggleBulkForm(bar, currentPath) {
  const existing = bar.querySelector(".bulk-form");
  if (existing) {
    existing.remove();
    return;
  }

  const select = el("select", { class: "cf-base" });
  for (const t of BASE_TYPES) {
    select.appendChild(el("option", { value: t }, t));
  }

  const tagsInput = el("textarea", {
    class: "cf-tags",
    rows: 2,
    placeholder: "tags applied to all — one per line",
  });

  const privacySel = el("select", { class: "cf-privacy" });
  privacySel.appendChild(el("option", { value: "" }, "— privacy —"));
  for (const p of ["public", "personal", "confidential"]) {
    privacySel.appendChild(el("option", { value: p }, p));
  }

  const roleSel = el("select", { class: "cf-role" });
  roleSel.appendChild(el("option", { value: "collection" }, "Collection (has children)"));
  roleSel.appendChild(el("option", { value: "item" }, "Item (atomic)"));

  const submit = el("button", { type: "button", class: "cf-submit" }, "Apply to all");
  const cancel = el("button", { type: "button", class: "cf-cancel" }, "Cancel");
  const msg = el("span", { class: "cf-msg muted" });

  submit.addEventListener("click", async () => {
    submit.disabled = true;
    msg.textContent = "Classifying…";
    msg.style.color = "";
    const body = {
      parent_path: currentPath,
      base_type: select.value,
      tags: tagsInput.value.split("\n").map(s => s.trim()).filter(Boolean),
      privacy: privacySel.value || undefined,
      is_item: roleSel.value === "item",
    };
    try {
      const res = await fetch("/api/unknowns/bulk-classify", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
      const r = await res.json();
      msg.textContent = `Classified ${r.classified}.`;
      msg.style.color = "var(--ok)";
      setTimeout(() => route(), 600);
    } catch (err) {
      msg.textContent = `Error: ${err.message}`;
      msg.style.color = "var(--warn)";
      submit.disabled = false;
    }
  });
  cancel.addEventListener("click", () => bar.querySelector(".bulk-form")?.remove());

  const form = el("div", { class: "bulk-form classify-form" },
    el("div", { class: "cf-row" },
      el("label", {}, "Role"), roleSel,
      el("label", {}, "Base type"), select,
      el("label", {}, "Privacy"), privacySel,
    ),
    el("div", { class: "cf-row" }, el("label", {}, "Tags"), tagsInput),
    el("div", { class: "cf-actions" }, submit, cancel, msg),
  );
  bar.appendChild(form);
}

const SCAN_OPTS_KEY = "fili.scan.opts";

function loadScanOpts() {
  try {
    const raw = localStorage.getItem(SCAN_OPTS_KEY);
    if (!raw) return { max_depth: "", index_files: false };
    const parsed = JSON.parse(raw);
    return {
      max_depth: parsed.max_depth == null ? "" : String(parsed.max_depth),
      index_files: !!parsed.index_files,
    };
  } catch {
    return { max_depth: "", index_files: false };
  }
}

function saveScanOpts(depthStr, indexFiles) {
  const trimmed = (depthStr || "").trim();
  const payload = {
    max_depth: trimmed === "" ? null : Math.max(0, parseInt(trimmed, 10)),
    index_files: !!indexFiles,
  };
  try { localStorage.setItem(SCAN_OPTS_KEY, JSON.stringify(payload)); } catch {}
  return payload;
}

function wireScanBar(currentPath) {
  const btn = view.querySelector("#scan-btn");
  const depthInput = view.querySelector("#scan-depth");
  const filesInput = view.querySelector("#scan-files");
  const openBtn = view.querySelector("#open-btn");
  const msg = view.querySelector("#scan-msg");
  const optsBtn = view.querySelector("#scan-opts-btn");
  const optsPopup = view.querySelector("#scan-opts-popup");

  // Hydrate options from localStorage and persist on change.
  if (depthInput && filesInput) {
    const opts = loadScanOpts();
    depthInput.value = opts.max_depth;
    filesInput.checked = opts.index_files;
    const persist = () => saveScanOpts(depthInput.value, filesInput.checked);
    depthInput.addEventListener("change", persist);
    depthInput.addEventListener("input", persist);
    filesInput.addEventListener("change", persist);
  }

  // Dropdown popup toggle + click-outside to close.
  if (optsBtn && optsPopup) {
    optsBtn.addEventListener("click", (ev) => {
      ev.stopPropagation();
      optsPopup.hidden = !optsPopup.hidden;
      optsBtn.classList.toggle("active", !optsPopup.hidden);
    });
    optsPopup.addEventListener("click", ev => ev.stopPropagation());
    document.addEventListener("click", () => {
      if (!optsPopup.hidden) {
        optsPopup.hidden = true;
        optsBtn.classList.remove("active");
      }
    });
  }

  if (!btn) return;

  if (openBtn) {
    openBtn.addEventListener("click", async () => {
      openBtn.disabled = true;
      try {
        const res = await fetch("/api/open", {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ path: currentPath }),
        });
        if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
      } catch (err) {
        msg.textContent = `Open failed: ${err.message}`;
        msg.style.color = "var(--warn)";
      } finally {
        openBtn.disabled = false;
      }
    });
  }

  btn.addEventListener("click", async () => {
    const { max_depth, index_files } = saveScanOpts(
      depthInput ? depthInput.value : "",
      filesInput ? filesInput.checked : false,
    );
    btn.disabled = true;
    msg.textContent = "Scanning…";
    msg.style.color = "";
    try {
      const res = await fetch("/api/scan", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ path: currentPath, max_depth, index_files }),
      });
      if (!res.ok) throw new Error(`${res.status}: ${await res.text()}`);
      const s = await res.json();
      const parts = [
        `${s.collections} collection${s.collections === 1 ? "" : "s"}`,
        `${s.items} item${s.items === 1 ? "" : "s"}`,
      ];
      if (index_files) parts.push(`${s.files} file${s.files === 1 ? "" : "s"}`);
      parts.push(`${s.unknowns} unknown${s.unknowns === 1 ? "" : "s"}`);
      msg.textContent = parts.join(", ");
      msg.style.color = "var(--ok)";
      setTimeout(() => { route(); loadSidebar(); }, 700);
    } catch (err) {
      msg.textContent = `Error: ${err.message}`;
      msg.style.color = "var(--warn)";
      btn.disabled = false;
    }
  });
}

const BASE_TYPES = [
  "ai-model", "application", "archive", "audio", "book", "boot",
  "build-artifact", "cache", "code", "config", "dependencies", "devices",
  "document", "emulator", "game", "gamedata", "generic", "home", "image",
  "inbox", "mount", "procfs", "services", "swap", "sysfs", "system",
  "video",
];

function renderEntryRows(e) {
  // is_item entries are atomic; collections nest. Badge on the row + a
  // different icon make the data-model distinction legible.
  const indexed = e.state === "indexed" && e.collection;
  const isItem = indexed && e.collection.is_item;
  const rowClass = `row row-${e.state}` + (isItem ? " row-item" : "");

  let icon;
  if (e.state === "file") icon = "📄";
  else if (e.state === "unknown") icon = "❓";
  else if (e.state === "unscanned") icon = "◻";
  else icon = typeEmoji(e.collection?.base_type) || "📁";

  const nameContent = e.is_dir
    ? el("a", { href: browseHref(e.path) }, e.name)
    : document.createTextNode(e.name);

  const nameCell = el("td", { class: "name" }, nameContent);

  if (indexed) {
    for (const t of e.collection.tags || []) {
      nameCell.appendChild(document.createTextNode(" "));
      nameCell.appendChild(tagChip(t));
    }
  }

  let kindText;
  if (indexed) {
    kindText = kindLabel(e.collection);
  } else if (e.state === "unknown") {
    kindText = "unknown";
  } else if (e.state === "unscanned") {
    kindText = "not scanned";
  } else {
    kindText = "file";
  }
  // Indexed rows get a clickable kind pill → search by base_type. Other
  // states don't carry a meaningful type to filter on, so stay as spans.
  const kindNode = indexed
    ? el("a", {
        class: `kind-pill kind-${e.state} kind-link`,
        href: `#/search?type=${encodeURIComponent(e.collection.base_type)}`,
        title: `Show all ${e.collection.base_type}`,
      }, kindText)
    : el("span", { class: `kind-pill kind-${e.state}` }, kindText);
  const kindCell = el("td", {}, kindNode);

  let info = "—";
  if (e.state === "unknown" && e.unknown) {
    const ext = (e.unknown.top_extensions || []).slice(0, 3)
      .map(x => `${x.ext}×${x.count}`).join(", ");
    info = ext ? ext : `${e.unknown.file_count}f / ${e.unknown.dir_count}d`;
  } else if (e.state === "file") {
    info = formatSize(e.size);
  }
  const infoCell = el("td", { class: "muted" }, info);

  const actionsCell = el("td", { class: "actions" });
  if (e.state === "unknown" && e.unknown) {
    actionsCell.appendChild(el("button", {
      class: "classify-btn",
      type: "button",
      onclick: (ev) => toggleClassifyForm(ev.target, e.unknown),
    }, "Classify"));
  }

  const mainRow = el("tr", { class: rowClass },
    el("td", { class: "icon" }, icon),
    nameCell,
    kindCell,
    infoCell,
    actionsCell
  );

  return [mainRow];
}

function toggleClassifyForm(button, unknown) {
  const mainRow = button.closest("tr");
  const next = mainRow.nextElementSibling;
  if (next && next.classList.contains("classify-form-row")) {
    next.remove();
    return;
  }
  const formRow = buildClassifyForm(unknown, mainRow);
  mainRow.after(formRow);
}

function buildClassifyForm(unknown, mainRow) {
  const select = el("select", { class: "cf-base", name: "base_type" });
  for (const t of BASE_TYPES) {
    select.appendChild(el("option", { value: t }, t));
  }

  const tagsInput = el("textarea", {
    class: "cf-tags",
    rows: 3,
    placeholder: "one tag per line — key or key=value\n(e.g. app=mytool)",
  });

  const privacySel = el("select", { class: "cf-privacy", name: "privacy" });
  privacySel.appendChild(el("option", { value: "" }, "— privacy —"));
  for (const p of ["public", "personal", "confidential"]) {
    privacySel.appendChild(el("option", { value: p }, p));
  }

  const roleSel = el("select", { class: "cf-role", name: "role" });
  roleSel.appendChild(el("option", { value: "collection" }, "Collection (has children)"));
  roleSel.appendChild(el("option", { value: "item" }, "Item (atomic)"));

  const submit = el("button", { type: "button", class: "cf-submit" }, "Save");
  const cancel = el("button", { type: "button", class: "cf-cancel" }, "Cancel");
  const msg = el("span", { class: "cf-msg muted" });

  submit.addEventListener("click", async () => {
    submit.disabled = true;
    msg.textContent = "";
    const tags = tagsInput.value
      .split("\n").map(s => s.trim()).filter(Boolean);
    const body = {
      base_type: select.value,
      tags,
      privacy: privacySel.value || undefined,
      is_item: roleSel.value === "item",
    };
    try {
      const res = await fetch(`/api/unknowns/${unknown.id}/classify`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(body),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`${res.status}: ${text}`);
      }
      // Success — reload the browse view
      route();
    } catch (err) {
      msg.textContent = `Error: ${err.message}`;
      submit.disabled = false;
    }
  });
  cancel.addEventListener("click", () => {
    mainRow.nextElementSibling?.remove();
  });

  const form = el("div", { class: "classify-form" },
    el("div", { class: "cf-row" },
      el("label", {}, "Role"), roleSel,
      el("label", {}, "Base type"), select,
      el("label", {}, "Privacy"), privacySel,
    ),
    el("div", { class: "cf-row" }, el("label", {}, "Tags"), tagsInput),
    el("div", { class: "cf-actions" }, submit, cancel, msg),
  );

  const cell = el("td", { colspan: "5", class: "classify-form-cell" }, form);
  return el("tr", { class: "classify-form-row" }, cell);
}

// ---------- Search (filter-based) ----------

async function showSearch(params) {
  mount("tpl-search");
  const form = view.querySelector("#filters");
  form.q.value = params.get("q") || "";
  form.tag.value = params.get("tag") || "";
  form.type.value = params.get("type") || "";
  form.privacy.value = params.get("privacy") || "";

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const next = new URLSearchParams();
    if (form.q.value) next.set("q", form.q.value);
    if (form.tag.value) next.set("tag", form.tag.value);
    if (form.type.value) next.set("type", form.type.value);
    if (form.privacy.value) next.set("privacy", form.privacy.value);
    location.hash = `#/search?${next.toString()}`;
  });

  const hasFilter =
    params.get("q") || params.get("tag") || params.get("type") || params.get("privacy");
  if (!hasFilter) {
    view.querySelector("#count").textContent = "Enter a query or pick a filter.";
    return;
  }

  const apiParams = new URLSearchParams();
  if (params.get("q")) apiParams.set("q", params.get("q"));
  if (params.get("tag")) apiParams.set("tag", params.get("tag"));
  if (params.get("type")) apiParams.set("type", params.get("type"));
  if (params.get("privacy")) apiParams.set("privacy", params.get("privacy"));
  apiParams.set("limit", "500");

  const rows = await fetchJson(`/api/collections?${apiParams.toString()}`);
  view.querySelector("#count").textContent = `${rows.length} collection(s)`;

  const tbody = view.querySelector("#collections-body");
  for (const c of rows) {
    const nameCell = el("td", {},
      el("a", { href: browseHref(c.path) }, basename(c.path) || c.path));
    for (const t of c.tags || []) {
      nameCell.appendChild(document.createTextNode(" "));
      nameCell.appendChild(tagChip(t));
    }
    tbody.appendChild(el("tr", {},
      nameCell,
      el("td", {}, el("a", {
        class: "kind-pill kind-indexed kind-link",
        href: `#/search?type=${encodeURIComponent(c.base_type)}`,
        title: `Show all ${c.base_type}`,
      }, c.base_type)),
      el("td", {}, c.privacy),
      el("td", {}, el("code", {}, c.path))
    ));
  }

  const table = tbody.closest("table");
  if (table) {
    // Columns: [Name, Type, Privacy, Path]
    makeSortable(table, [
      tr => tr.children[0]?.textContent?.trim() ?? "",
      tr => tr.children[1]?.textContent?.trim() ?? "",
      tr => tr.children[2]?.textContent?.trim() ?? "",
      tr => tr.children[3]?.textContent?.trim() ?? "",
    ]);
  }
}

// ---------- Overview ----------

async function showOverview() {
  mount("tpl-overview");
  const stats = await fetchJson("/api/stats");
  view.querySelector('[data-field="collection_count"]').textContent = stats.collection_count;
  view.querySelector('[data-field="unknown_count"]').textContent = stats.unknown_count ?? 0;
  view.querySelector('[data-field="unprotected_count"]').textContent = stats.unprotected_count;
  view.querySelector('[data-field="device_count"]').textContent = stats.device_count;
  view.querySelector('[data-field="location_count"]').textContent = stats.location_count;

  const tbody = view.querySelector('[data-field="by_type"]');
  for (const [type, count] of stats.by_type || []) {
    const tr = el("tr", {},
      el("td", {}, el("a", { href: `#/search?type=${encodeURIComponent(type)}` }, type)),
      el("td", {}, String(count))
    );
    tbody.appendChild(tr);
  }
}

// ---------- Drives ----------

async function showDrives() {
  mount("tpl-drives");
  const drives = await fetchJson("/api/drives");
  const tbody = view.querySelector("#drives-body");
  if (drives.length === 0) {
    tbody.appendChild(el("tr", {},
      el("td", { colspan: "9", class: "muted" },
        "No drives yet — run `fili scan` to detect them.")
    ));
    return;
  }
  for (const d of drives) {
    tbody.appendChild(renderDriveRow(d));
  }
}

function renderDriveRow(d) {
  const nameSpan = el("span", { class: "friendly-name" },
    d.friendly_name || d.label || "(unnamed)");
  const editBtn = el("button", {
    class: "rename-btn", type: "button",
    title: "Rename",
  }, "✏");
  const nameWrap = el("div", { class: "drive-name" }, nameSpan, editBtn);
  const nameCell = el("td", {}, nameWrap);
  editBtn.addEventListener("click", () => startRename(d, nameWrap));

  const mount = d.current_mount
    ? el("a", { href: browseHref(d.current_mount), title: "Browse this mount" },
        el("code", {}, d.current_mount))
    : el("span", { class: "muted" }, "not mounted");

  return el("tr", {},
    nameCell,
    el("td", {}, d.label || "—"),
    el("td", {}, mount),
    el("td", {}, d.fs_type || "—"),
    el("td", {}, d.size || "—"),
    el("td", {}, d.model || "—"),
    el("td", { class: "muted" }, d.serial || "—"),
    el("td", {}, el("code", {}, d.uuid || "—")),
    el("td", { class: "muted" }, formatTime(d.last_seen))
  );
}

function startRename(drive, wrap) {
  const input = el("input", {
    type: "text",
    value: drive.friendly_name || drive.label || "",
    placeholder: "friendly name",
    class: "rename-input",
  });
  const save = el("button", { type: "button", class: "rename-save" }, "Save");
  const cancel = el("button", { type: "button", class: "rename-cancel" }, "×");
  const original = wrap.cloneNode(true);
  // Re-wire the edit button on the cloned original so cancel restores a working row.
  const cloneBtn = original.querySelector(".rename-btn");
  if (cloneBtn) cloneBtn.addEventListener("click", () => startRename(drive, original));
  wrap.replaceChildren(input, save, cancel);
  input.focus();
  input.select();

  const commit = async () => {
    const value = input.value.trim();
    save.disabled = true;
    try {
      const res = await fetch(`/api/drives/${drive.id}/rename`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ friendly_name: value }),
      });
      if (!res.ok) throw new Error(`${res.status}`);
      route();
    } catch (err) {
      save.disabled = false;
      alert(`Rename failed: ${err.message}`);
    }
  };
  save.addEventListener("click", commit);
  input.addEventListener("keydown", (e) => {
    if (e.key === "Enter") commit();
    if (e.key === "Escape") wrap.replaceWith(original);
  });
  cancel.addEventListener("click", () => wrap.replaceWith(original));
}

// ---------- Locations ----------

async function showLocations() {
  mount("tpl-locations");
  const locations = await fetchJson("/api/locations");
  const tbody = view.querySelector("#locations-body");
  for (const l of locations) {
    const flagLabels = [];
    if (l.is_backup) flagLabels.push("backup");
    if (l.is_ephemeral) flagLabels.push("ephemeral");
    if (l.is_readonly) flagLabels.push("readonly");

    tbody.appendChild(el("tr", {},
      el("td", {}, el("a", { href: browseHref(l.path) }, l.name)),
      el("td", {}, el("code", {}, l.path)),
      el("td", {}, flagLabels.join(", ") || "—"),
      el("td", {}, formatTime(l.last_scan))
    ));
  }
}

// ---------- Router ----------

function parseHash() {
  const raw = (location.hash || "#/").slice(1);
  const [pathname, search = ""] = raw.split("?");
  return { pathname, params: new URLSearchParams(search) };
}

function setActiveNav(route) {
  for (const a of document.querySelectorAll("#side-nav a")) {
    a.classList.toggle("active", a.dataset.route === route);
  }
}

// Load sidebar shortcuts once at startup. /api/places gives home + XDG
// user dirs; /api/drives gives currently mounted drives. Both are cheap
// and render asynchronously so the main view isn't blocked.
async function loadSidebar() {
  const placesContainer = document.getElementById("side-places");
  const mountsContainer = document.getElementById("side-mounts");
  const recentContainer = document.getElementById("side-recent");

  try {
    const places = await fetchJson("/api/places");
    placesContainer.innerHTML = "";
    if (places.home) {
      placesContainer.appendChild(sidebarLink("🏠", "Home", places.home));
    }
    const userIcons = {
      Desktop: "🖥", Documents: "📄", Downloads: "📥",
      Music: "🎵", Pictures: "🖼", Videos: "🎬", Public: "🌐",
    };
    for (const p of places.user_dirs || []) {
      placesContainer.appendChild(sidebarLink(userIcons[p.label] || "📁", p.label, p.path));
    }
  } catch {
    placesContainer.innerHTML = "";
  }

  try {
    const drives = await fetchJson("/api/drives");
    mountsContainer.innerHTML = "";
    const mounted = (drives || []).filter(d => d.current_mount);
    if (mounted.length === 0) {
      mountsContainer.appendChild(el("a", { class: "side-loading" }, "none mounted"));
    } else {
      for (const d of mounted) {
        const label = d.friendly_name || d.label || basename(d.current_mount) || "drive";
        mountsContainer.appendChild(sidebarLink("💾", label, d.current_mount));
      }
    }
  } catch {
    mountsContainer.innerHTML = "";
  }

  // Recent = last ~7 scan roots, newest first. Uses last_scan when set,
  // otherwise falls back to id (legacy locations predate last_scan being
  // written, and a higher id is later in time since id autoincrements).
  try {
    const locations = await fetchJson("/api/locations");
    recentContainer.innerHTML = "";
    const keyOf = l => l.last_scan ?? l.id ?? 0;
    const sorted = (locations || [])
      .slice()
      .sort((a, b) => keyOf(b) - keyOf(a))
      .slice(0, 7);
    if (sorted.length === 0) {
      recentContainer.appendChild(el("a", { class: "side-loading" }, "none yet"));
      return;
    }
    for (const loc of sorted) {
      const label = basename(loc.path) || loc.path;
      recentContainer.appendChild(sidebarLink("🕘", label, loc.path));
    }
  } catch {
    recentContainer.innerHTML = "";
  }
}

function sidebarLink(icon, label, path) {
  return el("a",
    { href: browseHref(path), title: path },
    el("span", { class: "side-icon" }, icon),
    label,
  );
}

async function route() {
  const { pathname, params } = parseHash();
  try {
    if (pathname === "/" || pathname === "" || pathname === "/browse") {
      setActiveNav("browse");
      await showBrowse(params);
    } else if (pathname === "/search") {
      setActiveNav("search");
      await showSearch(params);
    } else if (pathname === "/overview") {
      setActiveNav("overview");
      await showOverview();
    } else if (pathname === "/drives") {
      setActiveNav("drives");
      await showDrives();
    } else if (pathname === "/locations") {
      setActiveNav("locations");
      await showLocations();
    } else {
      view.innerHTML = `<p class="loading">Not found: <code>${pathname}</code></p>`;
    }
  } catch (err) {
    view.innerHTML = `<p class="loading">Error: ${err.message}</p>`;
  }
}

window.addEventListener("hashchange", route);
route();
loadSidebar();
