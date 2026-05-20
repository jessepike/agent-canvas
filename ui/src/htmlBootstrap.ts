export const BOOTSTRAP_SCRIPT = String.raw`
(function () {
  const protocol = "agentcanvas-iframe/1";
  const highlightClass = "agentcanvas-comment-highlight";
  let selectionTimer = 0;

  function post(type, payload) {
    window.parent.postMessage(Object.assign({ type, protocol }, payload || {}), "*");
  }

  function stringify(value) {
    if (typeof value === "string") {
      return value;
    }
    try {
      return JSON.stringify(value);
    } catch {
      return String(value);
    }
  }

  function installStyle() {
    if (document.getElementById("agentcanvas-bootstrap-style")) {
      return;
    }
    const style = document.createElement("style");
    style.id = "agentcanvas-bootstrap-style";
    style.textContent = [
      ":root { --agentcanvas-comment-highlight-bg: Mark; }",
      "." + highlightClass + " { background: var(--agentcanvas-comment-highlight-bg); color: inherit; }"
    ].join("\n");
    document.head ? document.head.appendChild(style) : document.documentElement.appendChild(style);
  }

  function textOffsetForRangePoint(root, targetNode, targetOffset) {
    const prefix = document.createRange();
    prefix.selectNodeContents(root);
    prefix.setEnd(targetNode, targetOffset);
    return prefix.toString().length;
  }

  function publishSelection() {
    const range = currentSelectionRange();
    if (range) {
      post("agentcanvas:selection", { range });
    }
  }

  function currentSelectionRange() {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0 || selection.isCollapsed) {
      return null;
    }
    const range = selection.getRangeAt(0);
    const body = document.body;
    if (!body || !body.contains(range.commonAncestorContainer)) {
      return null;
    }
    const startOffset = textOffsetForRangePoint(body, range.startContainer, range.startOffset);
    const endOffset = textOffsetForRangePoint(body, range.endContainer, range.endOffset);
    return {
      startOffset: Math.min(startOffset, endOffset),
      endOffset: Math.max(startOffset, endOffset),
      text: selection.toString()
    };
  }

  function findTextRange(text) {
    if (!text || !document.body) {
      return null;
    }
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    const nodes = [];
    let fullText = "";
    let node = walker.nextNode();
    while (node) {
      const value = node.nodeValue || "";
      nodes.push({ node, start: fullText.length, end: fullText.length + value.length });
      fullText += value;
      node = walker.nextNode();
    }
    const start = fullText.indexOf(text);
    if (start === -1) {
      return null;
    }
    const end = start + text.length;
    const startEntry = nodes.find(function (entry) {
      return start >= entry.start && start <= entry.end;
    });
    const endEntry = nodes.find(function (entry) {
      return end >= entry.start && end <= entry.end;
    });
    if (!startEntry || !endEntry) {
      return null;
    }
    const range = document.createRange();
    range.setStart(startEntry.node, start - startEntry.start);
    range.setEnd(endEntry.node, end - endEntry.start);
    return range;
  }

  function scrollToSnapshot(text) {
    const range = findTextRange(text);
    if (!range) {
      return false;
    }
    const mark = document.createElement("mark");
    mark.className = highlightClass;
    try {
      range.surroundContents(mark);
    } catch {
      mark.appendChild(range.extractContents());
      range.insertNode(mark);
    }
    mark.scrollIntoView({ block: "center", inline: "nearest", behavior: "smooth" });
    window.setTimeout(function () {
      mark.replaceWith.apply(mark, Array.from(mark.childNodes));
    }, 1500);
    return true;
  }

  ["log", "info", "warn", "error"].forEach(function (level) {
    const original = console[level];
    console[level] = function () {
      const args = Array.from(arguments);
      post("agentcanvas:console", { level, message: args.map(stringify).join(" ") });
      original.apply(console, args);
    };
  });

  window.agentcanvas = {
    protocol,
    sendBack: function (payload) {
      post("agentcanvas:send_back", { payload: payload || {} });
      console.info("agentcanvas.sendBack received by host bridge");
    },
    scrollToSnapshot
  };

  document.addEventListener("selectionchange", function () {
    window.clearTimeout(selectionTimer);
    selectionTimer = window.setTimeout(publishSelection, 80);
  });
  document.addEventListener("keydown", function (event) {
    if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "m") {
      const range = currentSelectionRange();
      if (range) {
        event.preventDefault();
        post("agentcanvas:comment_shortcut", { range });
      }
    }
  });
  window.addEventListener("message", function (event) {
    if (event.data && event.data.type === "agentcanvas:scroll_to") {
      scrollToSnapshot(event.data.text || "");
    }
  });
  installStyle();
}());
`;

export function injectBootstrap(html: string): string {
  const script = `<script>${BOOTSTRAP_SCRIPT}</script>`;
  const headMatch = html.match(/<head(?:\s[^>]*)?>/i);
  if (!headMatch || headMatch.index === undefined) {
    return `${script}\n${html}`;
  }
  const insertAt = headMatch.index + headMatch[0].length;
  return `${html.slice(0, insertAt)}\n${script}${html.slice(insertAt)}`;
}
