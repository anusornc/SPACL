#!/usr/bin/env python3
"""Generate a self-contained interactive viewer for an OKF v0.2 bundle.

The bundle remains the source of truth. This script only reads Markdown
concepts, extracts their YAML frontmatter and internal Markdown links, and
embeds that data into a static HTML viewer.
"""

from __future__ import annotations

import argparse
import json
import posixpath
import re
from datetime import date, datetime
from pathlib import Path
from typing import Any

try:
    import yaml
except ImportError as exc:  # pragma: no cover - depends on the local Python env
    raise SystemExit("generate_okf_viz.py requires PyYAML (python3-yaml)") from exc


LINK_RE = re.compile(r"!?(?:\[[^\]]*\])\(([^)\s]+)(?:\s+[^)]*)?\)")
RESERVED = {"index.md", "log.md"}


def split_frontmatter(text: str, path: Path) -> tuple[dict[str, Any], str]:
    if not text.startswith("---\n"):
        raise ValueError(f"{path}: concept is missing YAML frontmatter")
    try:
        _, raw_frontmatter, body = text.split("---\n", 2)
    except ValueError as exc:
        raise ValueError(f"{path}: frontmatter is not closed") from exc
    metadata = yaml.safe_load(raw_frontmatter) or {}
    if not isinstance(metadata, dict) or not str(metadata.get("type", "")).strip():
        raise ValueError(f"{path}: frontmatter requires a non-empty type")
    return metadata, body.lstrip("\n")


def concept_id(path: Path, bundle: Path) -> str:
    relative = path.relative_to(bundle).as_posix()
    return relative[:-3] if relative.endswith(".md") else relative


def jsonable(value: Any) -> Any:
    """Convert YAML-native date values into JSON-safe display values."""
    if isinstance(value, (datetime, date)):
        return value.isoformat().replace("+00:00", "Z")
    if isinstance(value, dict):
        return {str(key): jsonable(item) for key, item in value.items()}
    if isinstance(value, list):
        return [jsonable(item) for item in value]
    return value


def resolve_link(source_id: str, href: str) -> str | None:
    href = href.split("#", 1)[0].split("?", 1)[0]
    if not href or href.startswith(("http://", "https://", "mailto:", "#")):
        return None
    if href == "viz.html" or href.endswith("/"):
        return None
    if href.startswith("/"):
        target = href[1:]
    else:
        target = posixpath.normpath(posixpath.join(posixpath.dirname(source_id), href))
    if target.endswith(".md"):
        target = target[:-3]
    return target.lstrip("./")


def read_bundle(bundle: Path) -> dict[str, Any]:
    nodes: list[dict[str, Any]] = []
    paths = sorted(path for path in bundle.rglob("*.md") if path.name not in RESERVED)

    for path in paths:
        metadata, body = split_frontmatter(path.read_text(encoding="utf-8"), path)
        node_id = concept_id(path, bundle)
        tags = metadata.get("tags", [])
        if isinstance(tags, str):
            tags = [tags]
        metadata = jsonable(metadata)
        nodes.append(
            {
                "id": node_id,
                "path": path.relative_to(bundle).as_posix(),
                "title": metadata.get("title") or node_id.rsplit("/", 1)[-1],
                "type": metadata["type"],
                "description": metadata.get("description", ""),
                "resource": metadata.get("resource", ""),
                "tags": tags,
                "frontmatter": metadata,
                "body": body,
            }
        )

    node_ids = {node["id"] for node in nodes}
    edges: list[dict[str, str]] = []
    seen: set[tuple[str, str]] = set()
    for node in nodes:
        for href in LINK_RE.findall(node["body"]):
            target = resolve_link(node["id"], href)
            if target in node_ids and (node["id"], target) not in seen:
                seen.add((node["id"], target))
                edges.append(
                    {
                        "id": f"edge-{len(edges)}",
                        "source": node["id"],
                        "target": target,
                    }
                )

    index_path = bundle / "index.md"
    version = "0.2"
    if index_path.exists():
        index_text = index_path.read_text(encoding="utf-8")
        if index_text.startswith("---\n"):
            try:
                _, raw_frontmatter, _ = index_text.split("---\n", 2)
                version = str((yaml.safe_load(raw_frontmatter) or {}).get("okf_version", version))
            except (ValueError, AttributeError):
                pass

    return {
        "name": "Tableauxx / SPACL-RRN",
        "version": version,
        "nodes": nodes,
        "edges": edges,
    }


HTML_TEMPLATE = r'''<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>__TITLE__ · OKF viewer</title>
  <script src="https://cdn.jsdelivr.net/npm/cytoscape@3.28.1/dist/cytoscape.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/marked@12.0.0/marked.min.js"></script>
  <style>
    :root {
      color-scheme: dark;
      --bg: #07111f;
      --panel: rgba(12, 26, 45, .88);
      --panel-strong: #0d2139;
      --line: rgba(151, 185, 220, .18);
      --muted: #8ea6bf;
      --text: #ecf5ff;
      --accent: #5eead4;
      --accent-2: #8ab4ff;
      --shadow: 0 20px 70px rgba(0, 0, 0, .28);
    }

    * { box-sizing: border-box; }
    html, body { height: 100%; }
    body {
      margin: 0;
      overflow: hidden;
      color: var(--text);
      background:
        radial-gradient(circle at 12% 0%, rgba(41, 98, 151, .35), transparent 38%),
        radial-gradient(circle at 100% 100%, rgba(21, 128, 113, .18), transparent 32%),
        var(--bg);
      font: 14px/1.55 ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    button, input, select { font: inherit; }
    button, select, input {
      color: var(--text);
      border: 1px solid var(--line);
      background: rgba(7, 17, 31, .72);
      border-radius: 9px;
    }
    button, select { cursor: pointer; }
    button:hover, select:hover { border-color: rgba(94, 234, 212, .58); }
    input:focus, select:focus, button:focus-visible {
      outline: 2px solid rgba(94, 234, 212, .75);
      outline-offset: 2px;
    }

    header {
      min-height: 76px;
      padding: 18px 26px 16px;
      border-bottom: 1px solid var(--line);
      background: rgba(7, 17, 31, .56);
      backdrop-filter: blur(18px);
      display: flex;
      align-items: center;
      justify-content: space-between;
      gap: 20px;
    }
    .brand { min-width: 0; }
    h1 { margin: 0; font-size: 20px; letter-spacing: -.02em; }
    .subtitle { margin-top: 2px; color: var(--muted); font-size: 12px; }
    .toolbar { display: flex; flex-wrap: wrap; justify-content: flex-end; gap: 8px; }
    input { width: min(29vw, 310px); padding: 9px 11px; }
    select, button { padding: 9px 11px; }

    main {
      height: calc(100vh - 76px);
      display: grid;
      grid-template-columns: minmax(0, 1fr) minmax(340px, 42%);
      gap: 1px;
      background: var(--line);
    }
    .graph-pane, .detail-pane { min-width: 0; background: var(--panel); }
    .graph-pane { position: relative; min-height: 0; }
    #graph { position: absolute; inset: 0; }
    .graph-hint {
      position: absolute;
      z-index: 2;
      left: 18px;
      bottom: 16px;
      max-width: 360px;
      padding: 9px 12px;
      color: var(--muted);
      border: 1px solid var(--line);
      border-radius: 10px;
      background: rgba(7, 17, 31, .68);
      pointer-events: none;
    }

    .detail-pane { overflow: auto; padding: 24px 25px 50px; }
    .detail-empty { color: var(--muted); padding-top: 42px; }
    .eyebrow { color: var(--accent); font-size: 11px; font-weight: 700; letter-spacing: .12em; text-transform: uppercase; }
    h2 { margin: 5px 0 7px; line-height: 1.2; font-size: 25px; letter-spacing: -.03em; }
    .description { margin: 0 0 15px; color: #c4d6e9; }
    .tags { display: flex; flex-wrap: wrap; gap: 6px; margin: 14px 0 21px; }
    .tag { padding: 3px 8px; color: #b9f7ef; border: 1px solid rgba(94, 234, 212, .3); border-radius: 999px; background: rgba(94, 234, 212, .08); font-size: 11px; }
    .section-title { margin: 22px 0 8px; color: var(--muted); font-size: 11px; font-weight: 700; letter-spacing: .1em; text-transform: uppercase; }
    .meta { width: 100%; border-collapse: collapse; font-size: 12px; }
    .meta th, .meta td { padding: 8px 0; border-bottom: 1px solid var(--line); vertical-align: top; text-align: left; }
    .meta th { width: 31%; color: var(--muted); font-weight: 600; }
    .meta td { color: #dceafa; overflow-wrap: anywhere; }
    .meta code { color: #b9f7ef; }
    .meta a, .markdown a { color: var(--accent-2); }
    .markdown { color: #d8e6f4; }
    .markdown h1, .markdown h2, .markdown h3 { margin: 24px 0 8px; color: var(--text); }
    .markdown h1 { font-size: 20px; }
    .markdown h2 { font-size: 17px; }
    .markdown h3 { font-size: 15px; }
    .markdown p { margin: 9px 0; }
    .markdown ul, .markdown ol { padding-left: 22px; }
    .markdown li { margin: 4px 0; }
    .markdown code { padding: 1px 4px; color: #b9f7ef; border-radius: 4px; background: rgba(0, 0, 0, .24); }
    .markdown pre { overflow: auto; padding: 13px; border: 1px solid var(--line); border-radius: 10px; background: rgba(0, 0, 0, .24); }
    .markdown pre code { padding: 0; background: transparent; }
    .markdown blockquote { margin-left: 0; padding-left: 14px; color: var(--muted); border-left: 2px solid var(--accent); }
    .backlinks { display: grid; gap: 6px; }
    .backlink { padding: 8px 10px; color: #cfe0f2; border: 1px solid var(--line); border-radius: 8px; background: rgba(255,255,255,.025); cursor: pointer; }
    .backlink:hover { border-color: rgba(138, 180, 255, .55); }
    .resource { overflow-wrap: anywhere; }
    .resource a { color: var(--accent-2); }
    .status {
      position: absolute;
      right: 17px;
      bottom: 16px;
      padding: 7px 10px;
      color: var(--muted);
      border: 1px solid var(--line);
      border-radius: 9px;
      background: rgba(7, 17, 31, .68);
      font-size: 12px;
      pointer-events: none;
    }

    @media (max-width: 900px) {
      body { overflow: auto; }
      header { align-items: flex-start; flex-direction: column; }
      .toolbar { width: 100%; justify-content: flex-start; }
      input { width: min(100%, 340px); }
      main { height: auto; min-height: calc(100vh - 134px); grid-template-columns: 1fr; }
      .graph-pane { height: 58vh; min-height: 420px; }
      .detail-pane { min-height: 500px; }
    }
  </style>
</head>
<body>
  <header>
    <div class="brand">
      <h1>__TITLE__</h1>
      <div class="subtitle">Open Knowledge Format v__VERSION__ · <span id="count"></span></div>
    </div>
    <div class="toolbar" aria-label="Graph controls">
      <input id="search" type="search" placeholder="Search concepts, tags, or paths" aria-label="Search concepts">
      <select id="type-filter" aria-label="Filter by type"><option value="">All types</option></select>
      <select id="layout" aria-label="Graph layout">
        <option value="cose">Force-directed</option>
        <option value="breadthfirst">Breadth-first</option>
        <option value="concentric">Concentric</option>
        <option value="circle">Circle</option>
        <option value="grid">Grid</option>
      </select>
      <button id="fit" type="button">Fit graph</button>
    </div>
  </header>
  <main>
    <section class="graph-pane" aria-label="Knowledge graph">
      <div id="graph"></div>
      <div class="graph-hint">Click a node to inspect the concept. Edges are Markdown links between OKF documents.</div>
      <div class="status" id="status"></div>
    </section>
    <aside class="detail-pane" id="detail" aria-live="polite">
      <div class="detail-empty">Select a concept in the graph to browse its frontmatter, body, and backlinks.</div>
    </aside>
  </main>

  <script>
    const BUNDLE = __DATA__;
    const nodesById = new Map(BUNDLE.nodes.map(node => [node.id, node]));
    const incoming = new Map(BUNDLE.nodes.map(node => [node.id, []]));
    BUNDLE.edges.forEach(edge => {
      if (incoming.has(edge.target)) incoming.get(edge.target).push(edge.source);
    });

    const colors = ["#5eead4", "#8ab4ff", "#f6c453", "#ef8dff", "#ff8f8f", "#9be15d", "#f6a96b", "#aab2ff"];
    const types = [...new Set(BUNDLE.nodes.map(node => node.type))].sort();
    const typeColors = new Map(types.map((type, index) => [type, colors[index % colors.length]]));
    const filter = document.getElementById("type-filter");
    types.forEach(type => {
      const option = document.createElement("option");
      option.value = type;
      option.textContent = type;
      filter.appendChild(option);
    });

    const cy = cytoscape({
      container: document.getElementById("graph"),
      elements: [
        ...BUNDLE.nodes.map(node => ({ data: { id: node.id, label: node.title, type: node.type, color: typeColors.get(node.type) } })),
        ...BUNDLE.edges.map(edge => ({ data: edge }))
      ],
      minZoom: .25,
      maxZoom: 3,
      wheelSensitivity: .18,
      style: [
        { selector: "node", style: {
          "background-color": "data(color)", "label": "data(label)", "color": "#ecf5ff",
          "font-size": 10, "font-weight": 600, "text-wrap": "wrap", "text-max-width": 120,
          "text-valign": "center", "text-halign": "center", "text-outline-width": 2,
          "text-outline-color": "#07111f", "width": 34, "height": 34, "border-width": 2,
          "border-color": "rgba(255,255,255,.65)", "overlay-opacity": 0
        } },
        { selector: "node:selected", style: { "width": 46, "height": 46, "border-width": 4, "border-color": "#ffffff", "font-size": 11 } },
        { selector: "edge", style: {
          "curve-style": "bezier", "line-color": "rgba(151,185,220,.35)", "target-arrow-color": "rgba(151,185,220,.65)",
          "target-arrow-shape": "triangle", "arrow-scale": .7, "width": 1.2, "opacity": .78
        } },
        { selector: ".hidden", style: { "display": "none" } }
      ],
      layout: { name: "cose", animate: false, padding: 50, idealEdgeLength: 120, nodeRepulsion: 8500, gravity: .35 }
    });

    function escapeHtml(value) {
      return String(value ?? "").replace(/[&<>'"]/g, character => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", "'": "&#39;", '"': "&quot;" }[character]));
    }

    function formatValue(value) {
      if (value === null || value === undefined) return "";
      if (typeof value === "object") return JSON.stringify(value);
      return String(value);
    }

    function resourceHtml(value) {
      const text = escapeHtml(value);
      if (/^https?:\/\//.test(String(value))) return `<a href="${text}" target="_blank" rel="noopener noreferrer">${text}</a>`;
      return text;
    }

    function resolveConceptId(sourceId, href) {
      const clean = String(href || "").split("#", 1)[0].split("?", 1)[0];
      if (!clean || /^(?:https?:|mailto:|javascript:)/i.test(clean)) return null;
      let target;
      if (clean.startsWith("/")) target = clean.slice(1);
      else {
        const parts = sourceId.split("/");
        parts.pop();
        clean.split("/").forEach(part => {
          if (!part || part === ".") return;
          if (part === "..") parts.pop();
          else parts.push(part);
        });
        target = parts.join("/");
      }
      if (target.endsWith(".md")) target = target.slice(0, -3);
      return nodesById.has(target) ? target : null;
    }

    function renderBody(node) {
      const rendered = typeof marked !== "undefined" ? marked.parse(node.body) : `<pre>${escapeHtml(node.body)}</pre>`;
      const wrapper = document.createElement("div");
      wrapper.innerHTML = rendered;
      wrapper.querySelectorAll("a").forEach(anchor => {
        const target = resolveConceptId(node.id, anchor.getAttribute("href"));
        if (target) {
          anchor.href = "#";
          anchor.dataset.conceptId = target;
          anchor.addEventListener("click", event => { event.preventDefault(); selectConcept(target); });
        } else {
          anchor.target = "_blank";
          anchor.rel = "noopener noreferrer";
        }
      });
      return wrapper.innerHTML;
    }

    function selectConcept(id) {
      const node = nodesById.get(id);
      if (!node) return;
      cy.nodes().unselect();
      const graphNode = cy.getElementById(id);
      if (graphNode.length) { graphNode.select(); cy.animate({ center: { eles: graphNode }, duration: 250 }); }
      const frontmatterRows = Object.entries(node.frontmatter).map(([key, value]) => {
        const formatted = formatValue(value);
        return `<tr><th>${escapeHtml(key)}</th><td class="${key === "resource" ? "resource" : ""}">${key === "resource" ? resourceHtml(formatted) : escapeHtml(formatted)}</td></tr>`;
      }).join("");
      const tagHtml = (node.tags || []).map(tag => `<span class="tag">${escapeHtml(tag)}</span>`).join("");
      const backlinks = (incoming.get(id) || []).map(source => `<button class="backlink" data-concept-id="${escapeHtml(source)}">${escapeHtml(nodesById.get(source)?.title || source)}</button>`).join("");
      document.getElementById("detail").innerHTML = `
        <div class="eyebrow">${escapeHtml(node.type)}</div>
        <h2>${escapeHtml(node.title)}</h2>
        <p class="description">${escapeHtml(node.description)}</p>
        <div class="tags">${tagHtml}</div>
        <div class="section-title">Frontmatter</div>
        <table class="meta"><tbody>${frontmatterRows}</tbody></table>
        <div class="section-title">Knowledge</div>
        <div class="markdown">${renderBody(node)}</div>
        ${backlinks ? `<div class="section-title">Cited by / linked from</div><div class="backlinks">${backlinks}</div>` : ""}
      `;
      document.querySelectorAll(".backlink").forEach(button => button.addEventListener("click", () => selectConcept(button.dataset.conceptId)));
    }

    function applyFilters() {
      const query = document.getElementById("search").value.trim().toLowerCase();
      const selectedType = filter.value;
      const visible = new Set();
      BUNDLE.nodes.forEach(node => {
        const haystack = [node.id, node.title, node.type, node.description, ...(node.tags || [])].join(" ").toLowerCase();
        const matches = (!query || haystack.includes(query)) && (!selectedType || node.type === selectedType);
        if (matches) visible.add(node.id);
        cy.getElementById(node.id).toggleClass("hidden", !matches);
      });
      cy.edges().forEach(edge => edge.toggleClass("hidden", !visible.has(edge.data("source")) || !visible.has(edge.data("target"))));
      document.getElementById("status").textContent = `${visible.size} of ${BUNDLE.nodes.length} concepts · ${BUNDLE.edges.length} links`;
    }

    function relayout() {
      const name = document.getElementById("layout").value;
      const options = { name, animate: true, animationDuration: 350, padding: 50 };
      if (name === "cose") Object.assign(options, { idealEdgeLength: 120, nodeRepulsion: 8500, gravity: .35 });
      if (name === "concentric") Object.assign(options, { concentric: node => node.degree(), levelWidth: () => 2 });
      cy.layout(options).run();
    }

    document.getElementById("search").addEventListener("input", applyFilters);
    filter.addEventListener("change", applyFilters);
    document.getElementById("layout").addEventListener("change", relayout);
    document.getElementById("fit").addEventListener("click", () => cy.fit(cy.elements().not(".hidden"), 50));
    cy.on("tap", "node", event => selectConcept(event.target.id()));
    document.getElementById("count").textContent = `${BUNDLE.nodes.length} concepts · ${BUNDLE.edges.length} links`;
    applyFilters();
    selectConcept(BUNDLE.nodes.find(node => node.id === "project")?.id || BUNDLE.nodes[0]?.id);
  </script>
</body>
</html>
'''


def html_for(bundle_data: dict[str, Any], name: str) -> str:
    payload = json.dumps(bundle_data, ensure_ascii=False, separators=(",", ":"))
    payload = payload.replace("</", "<\\/").replace("\u2028", "\\u2028").replace("\u2029", "\\u2029")
    return (
        HTML_TEMPLATE.replace("__TITLE__", name)
        .replace("__VERSION__", str(bundle_data["version"]))
        .replace("__DATA__", payload)
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bundle", type=Path, default=Path("okf"), help="OKF bundle root")
    parser.add_argument("--out", type=Path, help="HTML output path (default: <bundle>/viz.html)")
    parser.add_argument("--name", default="Tableauxx / SPACL-RRN", help="Viewer header name")
    args = parser.parse_args()

    bundle = args.bundle.resolve()
    output = (args.out or bundle / "viz.html").resolve()
    data = read_bundle(bundle)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(html_for(data, args.name), encoding="utf-8")
    print(f"wrote {output} ({len(data['nodes'])} concepts, {len(data['edges'])} links)")


if __name__ == "__main__":
    main()
