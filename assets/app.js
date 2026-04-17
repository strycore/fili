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
  generic: "📄",
};

function iconForType(type) {
  return TYPE_ICONS[type] || "📄";
}

// Derive a human label from the collection's tags; falls back to base_type.
// `library`, `store=X`, `platform=X` are treated as structural markers.
function kindLabel(c) {
  const tags = c.tags || [];
  const byKey = Object.fromEntries(tags.map(t => [t.key, t.value]));
  if ("library" in byKey) return "Library";
  if ("store" in byKey) return `Store: ${byKey.store}`;
  if ("platform" in byKey) return `Platform: ${byKey.platform}`;
  if ("collection" in byKey) return "Collection";
  const isGrouping = (c.descendant_count || 0) > 0;
  if (isGrouping) return `${c.base_type} (group)`;
  return c.base_type;
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
  const path = params.get("path") || "";
  const url = path ? `/api/browse?path=${encodeURIComponent(path)}` : "/api/browse";
  const data = await fetchJson(url);

  // Breadcrumbs
  const crumbs = view.querySelector("#breadcrumbs");
  crumbs.appendChild(el("a", { href: "#/" }, "roots"));
  const chain = [...data.ancestors];
  if (data.current) chain.push(data.current);
  for (const c of chain) {
    crumbs.appendChild(el("span", { class: "sep" }, "›"));
    crumbs.appendChild(el("a", { href: browseHref(c.path) }, basename(c.path) || c.path));
  }

  // Current collection metadata (if we're inside one)
  if (data.current) {
    const cur = data.current;
    const metaBox = view.querySelector("#current");
    metaBox.hidden = false;
    metaBox.querySelector('[data-field="base_type"]').textContent = cur.base_type;
    metaBox.querySelector('[data-field="size"]').textContent = formatSize(cur.total_size);
    metaBox.querySelector('[data-field="file_count"]').textContent = `${cur.file_count} files`;
    metaBox.querySelector('[data-field="privacy"]').textContent = cur.privacy;

    const tagsDiv = metaBox.querySelector('[data-field="tags"]');
    for (const t of cur.tags || []) {
      const label = t.value ? `${t.key}=${t.value}` : t.key;
      tagsDiv.appendChild(el("span", { class: "tag" }, label));
    }
  }

  // Children (direct path-children in the indexed tree)
  const childrenBody = view.querySelector("#children-body");
  view.querySelector("#children-count").textContent = `(${data.children.length})`;
  if (data.children.length === 0) {
    view.querySelector("#children-section").hidden = true;
  }
  const basePath = data.path || "";
  for (const c of data.children) {
    const rel = relativeSegment(c.path, basePath);
    const isGrouping = (c.descendant_count || 0) > 0;
    const icon = isGrouping ? "📁" : iconForType(c.base_type);
    const kind = kindLabel(c);

    const nameCell = el("td", {},
      el("a", { href: browseHref(c.path) }, rel)
    );
    for (const t of c.tags || []) {
      nameCell.appendChild(document.createTextNode(" "));
      const label = t.value ? `${t.key}=${t.value}` : t.key;
      nameCell.appendChild(el("span", { class: "tag" }, label));
    }

    const contains = isGrouping
      ? `${c.descendant_count} item${c.descendant_count === 1 ? "" : "s"}`
      : (c.file_count ? `${c.file_count} file${c.file_count === 1 ? "" : "s"}` : "—");

    childrenBody.appendChild(el("tr", { class: isGrouping ? "row-grouping" : "row-leaf" },
      el("td", { class: "icon" }, icon),
      nameCell,
      el("td", {}, el("span", { class: "kind-pill" }, kind)),
      el("td", { class: "muted" }, contains),
      el("td", {}, formatSize(c.total_size))
    ));
  }

  // Files (only shown when we're in a collection)
  if (data.current && data.files.length > 0) {
    view.querySelector("#files-section").hidden = false;
    view.querySelector("#files-count").textContent =
      `(${data.files.length}${data.files.length === 500 ? "+" : ""})`;
    const filesBody = view.querySelector("#files-body");
    for (const f of data.files) {
      const rel = relativeSegment(f.path, data.current.path);
      filesBody.appendChild(el("tr", {},
        el("td", {}, el("code", {}, rel)),
        el("td", {}, f.base_type || "—"),
        el("td", {}, formatTime(f.mtime))
      ));
    }
  }

  // Empty state
  if (data.children.length === 0 && (!data.current || data.files.length === 0)) {
    const section = view.querySelector(".browse");
    section.appendChild(el("p", { class: "loading" },
      path ? "Nothing indexed under this path." : "No collections indexed. Run `fili scan`."));
  }
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
      el("td", {}, formatSize(c.total_size)),
      el("td", {}, String(c.file_count)),
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
  view.querySelector('[data-field="file_count"]').textContent = stats.file_count;
  view.querySelector('[data-field="total_size_human"]').textContent = formatSize(stats.total_size);
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
