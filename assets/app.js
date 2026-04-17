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

const TYPE_ICONS = {
  image: "🖼",
  audio: "🎵",
  video: "🎬",
  game: "🎮",
  application: "📦",
  document: "📄",
  code: "📜",
  archive: "🗜",
  cache: "🧹",
  home: "🏠",
  generic: "📄",
};

function iconForType(type) {
  return TYPE_ICONS[type] || "📄";
}

// Derive a human label from the collection's tags; falls back to base_type.
// Structural markers (library, store=X, platform=X, contains=X, mounts=X)
// are surfaced as the kind; otherwise we show the base type.
function kindLabel(c) {
  const tags = c.tags || [];
  const byKey = Object.fromEntries(tags.map(t => [t.key, t.value]));
  if ("library" in byKey) return "Library";
  if ("store" in byKey) return `Store: ${byKey.store}`;
  if ("platform" in byKey) return `Platform: ${byKey.platform}`;
  if ("vendor" in byKey) return `Vendor: ${byKey.vendor}`;
  if ("contains" in byKey) return capitalize(byKey.contains);
  if ("mounts" in byKey) return `${capitalize(byKey.mounts)} mounts`;
  if ("collection" in byKey) return "Collection";
  return c.base_type;
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
  const crumbs = view.querySelector("#breadcrumbs");
  crumbs.appendChild(el("a", { href: browseHref("/") }, "/"));
  const parts = (data.path || "/").split("/").filter(Boolean);
  let acc = "";
  for (const part of parts) {
    acc += "/" + part;
    crumbs.appendChild(el("span", { class: "sep" }, "›"));
    crumbs.appendChild(el("a", { href: browseHref(acc) }, part));
  }

  // Current collection metadata (if we're inside one)
  if (data.current) {
    const cur = data.current;
    const metaBox = view.querySelector("#current");
    metaBox.hidden = false;
    metaBox.querySelector('[data-field="base_type"]').textContent = cur.base_type;
    metaBox.querySelector('[data-field="privacy"]').textContent = cur.privacy;

    const tagsDiv = metaBox.querySelector('[data-field="tags"]');
    for (const t of cur.tags || []) {
      const label = t.value ? `${t.key}=${t.value}` : t.key;
      tagsDiv.appendChild(el("span", { class: "tag" }, label));
    }
  }

  const entriesBody = view.querySelector("#entries-body");
  view.querySelector("#entries-count").textContent = `(${data.entries.length})`;

  if (data.entries.length === 0) {
    const section = view.querySelector(".browse");
    section.appendChild(el("p", { class: "loading" },
      "This directory is empty or not readable."));
    return;
  }

  for (const e of data.entries) {
    entriesBody.appendChild(renderEntry(e));
  }
}

function renderEntry(e) {
  const rowClass = `row row-${e.state}`;
  let icon;
  if (e.state === "file") icon = "📄";
  else if (e.state === "unknown") icon = "❓";
  else if (e.state === "unscanned") icon = "◻";
  else icon = "📁"; // collection

  // Name column: link for dirs, plain text for files (we don't browse into files)
  const nameContent = e.is_dir
    ? el("a", { href: browseHref(e.path) }, e.name)
    : document.createTextNode(e.name);

  const nameCell = el("td", { class: "name" }, nameContent);

  // Show tags on classified collections
  if (e.state === "collection" && e.collection) {
    for (const t of e.collection.tags || []) {
      nameCell.appendChild(document.createTextNode(" "));
      const label = t.value ? `${t.key}=${t.value}` : t.key;
      nameCell.appendChild(el("span", { class: "tag" }, label));
    }
  }

  // Kind column: describes DB state + (if classified) kind label
  let kindText;
  if (e.state === "collection" && e.collection) {
    kindText = kindLabel(e.collection);
  } else if (e.state === "unknown") {
    kindText = "unknown";
  } else if (e.state === "unscanned") {
    kindText = "not scanned";
  } else {
    kindText = "file";
  }
  const kindCell = el("td", {}, el("span", { class: `kind-pill kind-${e.state}` }, kindText));

  // Info column: unknown preview extensions, or file size/mtime, or empty
  let info = "—";
  if (e.state === "unknown" && e.unknown) {
    const ext = (e.unknown.top_extensions || []).slice(0, 3)
      .map(x => `${x.ext}×${x.count}`).join(", ");
    info = ext ? ext : `${e.unknown.file_count}f / ${e.unknown.dir_count}d`;
  } else if (e.state === "file") {
    info = formatSize(e.size);
  }
  const infoCell = el("td", { class: "muted" }, info);

  return el("tr", { class: rowClass },
    el("td", { class: "icon" }, icon),
    nameCell,
    kindCell,
    infoCell
  );
}

// ---------- Search (filter-based) ----------

async function showSearch(params) {
  mount("tpl-search");
  const form = view.querySelector("#filters");
  form.q.value = params.get("q") || "";
  form.type.value = params.get("type") || "";
  form.privacy.value = params.get("privacy") || "";

  form.addEventListener("submit", (e) => {
    e.preventDefault();
    const next = new URLSearchParams();
    if (form.q.value) next.set("q", form.q.value);
    if (form.type.value) next.set("type", form.type.value);
    if (form.privacy.value) next.set("privacy", form.privacy.value);
    location.hash = `#/search?${next.toString()}`;
  });

  const hasFilter = params.get("q") || params.get("type") || params.get("privacy");
  if (!hasFilter) {
    view.querySelector("#count").textContent = "Enter a query or pick a filter.";
    return;
  }

  const apiParams = new URLSearchParams();
  if (params.get("q")) apiParams.set("q", params.get("q"));
  if (params.get("type")) apiParams.set("type", params.get("type"));
  if (params.get("privacy")) apiParams.set("privacy", params.get("privacy"));
  apiParams.set("limit", "500");

  const rows = await fetchJson(`/api/collections?${apiParams.toString()}`);
  view.querySelector("#count").textContent = `${rows.length} collection(s)`;

  const tbody = view.querySelector("#collections-body");
  for (const c of rows) {
    tbody.appendChild(el("tr", {},
      el("td", {}, el("a", { href: browseHref(c.path) }, basename(c.path) || c.path)),
      el("td", {}, c.base_type),
      el("td", {}, c.privacy),
      el("td", {}, el("code", {}, c.path))
    ));
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
  for (const a of document.querySelectorAll("header nav a")) {
    a.classList.toggle("active", a.dataset.route === route);
  }
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
