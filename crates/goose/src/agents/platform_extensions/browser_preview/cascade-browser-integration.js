// Browser-preview integration script, injected into proxied pages by the
// Cascade (Go) and Devin CLI (Rust) preview proxies. Served at
// /cascade-browser-integration.js on the proxy origin.
//
// Hand-written, dependency-free plain JavaScript: no bundler or node toolchain
// produces this file — both proxies embed it as-is. It talks Connect unary
// over binary protobuf to `exa.browser_preview_pb.BrowserPreviewService`
// (browser_preview.proto / browser_context.proto); the message encoders below
// must stay in sync with those protos.
(() => {
  "use strict";

  const SERVICE_PATH = "/exa.browser_preview_pb.BrowserPreviewService";
  const CSRF_HEADER = "x-codeium-csrf-token";
  const MAX_Z_INDEX = 2147483647;
  // Cap the retained console errors so a page stuck in an error loop can't
  // grow an unbounded buffer (and thus an unbounded capture payload); the
  // oldest entries are dropped once the cap is reached.
  const MAX_CONSOLE_ENTRIES = 100;

  // ---------------------------------------------------------------------------
  // Protobuf encoding
  //
  // Proto3 scalar fields are omitted when they hold the default value, matching
  // what the generated protobuf-es client serialized.
  // ---------------------------------------------------------------------------

  const utf8 = new TextEncoder();

  class ProtoWriter {
    constructor() {
      /** @type {Uint8Array[]} */
      this.chunks = [];
      this.length = 0;
    }

    /** @param {Uint8Array} chunk */
    bytes(chunk) {
      this.chunks.push(chunk);
      this.length += chunk.length;
    }

    /** @param {number} value */
    varint(value) {
      const out = [];
      let n = value >>> 0;
      do {
        let byte = n & 0x7f;
        n >>>= 7;
        if (n !== 0) {
          byte |= 0x80;
        }
        out.push(byte);
      } while (n !== 0);
      this.bytes(Uint8Array.from(out));
    }

    /**
     * @param {number} field
     * @param {number} wireType
     */
    tag(field, wireType) {
      this.varint((field << 3) | wireType);
    }

    /**
     * @param {number} field
     * @param {string | undefined} value
     */
    string(field, value) {
      if (!value) {
        return;
      }
      const encoded = utf8.encode(value);
      this.tag(field, 2);
      this.varint(encoded.length);
      this.bytes(encoded);
    }

    /**
     * @param {number} field
     * @param {number | undefined} value
     */
    uint32(field, value) {
      if (!value) {
        return;
      }
      this.tag(field, 0);
      this.varint(value);
    }

    /**
     * @param {number} field
     * @param {Uint8Array} encoded
     */
    message(field, encoded) {
      this.tag(field, 2);
      this.varint(encoded.length);
      this.bytes(encoded);
    }

    finish() {
      const out = new Uint8Array(this.length);
      let offset = 0;
      for (const chunk of this.chunks) {
        out.set(chunk, offset);
        offset += chunk.length;
      }
      return out;
    }
  }

  /**
   * @typedef {{ absoluteUri: string, startLine?: number }} FileLineRange
   * @typedef {{
   *   tagName: string,
   *   outerHtml: string,
   *   id?: string,
   *   reactComponentName?: string,
   *   fileLineRange?: FileLineRange,
   * }} DomElementScopeItem
   * @typedef {{ timestampStr: string, type: string, output: string }} ConsoleLogLine
   */

  // codeium_common_pb.FileLineRange
  /** @param {FileLineRange} range */
  function encodeFileLineRange({ absoluteUri, startLine }) {
    const w = new ProtoWriter();
    w.string(1, absoluteUri);
    w.uint32(2, startLine);
    return w.finish();
  }

  // codeium_common_pb.DOMElementScopeItem
  /** @param {DomElementScopeItem} item */
  function encodeDomElement({
    tagName,
    outerHtml,
    id,
    reactComponentName,
    fileLineRange,
  }) {
    const w = new ProtoWriter();
    w.string(1, tagName);
    w.string(2, outerHtml);
    w.string(3, id);
    w.string(4, reactComponentName);
    if (fileLineRange) {
      w.message(5, encodeFileLineRange(fileLineRange));
    }
    return w.finish();
  }

  // codeium_common_pb.ConsoleLogLine
  /** @param {ConsoleLogLine} line */
  function encodeConsoleLogLine({ timestampStr, type, output }) {
    const w = new ProtoWriter();
    w.string(1, timestampStr);
    w.string(2, type);
    w.string(3, output);
    return w.finish();
  }

  // codeium_common_pb.ConsoleLogScopeItem
  /** @param {{ lines: ConsoleLogLine[], serverAddress: string }} item */
  function encodeConsoleLog({ lines, serverAddress }) {
    const w = new ProtoWriter();
    for (const line of lines) {
      w.message(1, encodeConsoleLogLine(line));
    }
    w.string(2, serverAddress);
    return w.finish();
  }

  // ---------------------------------------------------------------------------
  // Connect client
  // ---------------------------------------------------------------------------

  // Resolved lazily: some proxies inject the CSRF meta tag after this script
  // tag, so the meta may not exist yet at script-eval time.
  function csrfToken() {
    const meta = /** @type {HTMLMetaElement | null} */ (
      document.querySelector(`meta[name="${CSRF_HEADER}"]`)
    );
    return meta?.content || "";
  }

  /**
   * @param {string} method
   * @param {Uint8Array} requestField
   */
  async function rpc(method, requestField) {
    const w = new ProtoWriter();
    w.message(1, requestField);
    /** @type {Record<string, string>} */
    const headers = {
      "content-type": "application/proto",
      "connect-protocol-version": "1",
    };
    const token = csrfToken();
    if (token) {
      headers[CSRF_HEADER] = token;
    }
    // Resolve against the proxy origin explicitly: a relative URL would
    // resolve against document.baseURI, which a page's <base href> can point
    // at a foreign origin.
    const response = await fetch(
      new URL(`${SERVICE_PATH}/${method}`, window.location.origin),
      {
        method: "POST",
        headers,
        body: w.finish(),
      }
    );
    if (!response.ok) {
      throw new Error(`${method} failed with status ${response.status}`);
    }
  }

  // ---------------------------------------------------------------------------
  // Source maps
  //
  // A minimal decoder for the standard (non-indexed) source-map format: enough
  // to answer originalPositionFor() the way the source-map library did for the
  // stack-trace positions below.
  // ---------------------------------------------------------------------------

  const VLQ_INDEX = (() => {
    const chars =
      "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    const index = new Map();
    for (let i = 0; i < chars.length; i++) {
      index.set(chars[i], i);
    }
    return index;
  })();

  // Decodes one base64-VLQ value; returns [value, nextPosition].
  /**
   * @param {string} text
   * @param {number} position
   * @returns {[number, number]}
   */
  function decodeVlq(text, position) {
    let result = 0;
    let shift = 0;
    for (;;) {
      const digit = VLQ_INDEX.get(text[position]);
      if (digit === undefined) {
        throw new Error("invalid VLQ");
      }
      position++;
      result += (digit & 0x1f) << shift;
      if ((digit & 0x20) === 0) {
        break;
      }
      shift += 5;
    }
    const negative = (result & 1) === 1;
    result >>>= 1;
    return [negative ? -result : result, position];
  }

  // Returns { source, line, column } (line 1-based, like the source-map
  // library) or null. `line` is 1-based and `column` is as reported by the
  // stack frame. Picks the last segment on the line whose generated column
  // does not exceed `column` (the library's greatest-lower-bound bias).
  /** @typedef {{ mappings: string, sources: string[], sourceRoot?: string }} SourceMapPayload */

  /**
   * @param {SourceMapPayload} map
   * @param {number} line
   * @param {number} column
   * @returns {{ source: string, line: number, column: number } | null}
   */
  function originalPositionFor(map, line, column) {
    if (typeof map.mappings !== "string" || !Array.isArray(map.sources)) {
      return null;
    }
    const lines = map.mappings.split(";");
    const mappingLine = lines[line - 1];
    if (!mappingLine) {
      return null;
    }

    // Accumulate the per-line-reset generated column and the absolute
    // source/line/column state from the start of the mappings.
    let sourceIndex = 0;
    let sourceLine = 0;
    let sourceColumn = 0;
    let best = null;
    for (let i = 0; i < line; i++) {
      let generatedColumn = 0;
      const segments = lines[i];
      let position = 0;
      while (position < segments.length) {
        const end = segments.indexOf(",", position);
        const segmentEnd = end === -1 ? segments.length : end;
        if (segmentEnd > position) {
          let value;
          [value, position] = decodeVlq(segments, position);
          generatedColumn += value;
          if (position < segmentEnd) {
            [value, position] = decodeVlq(segments, position);
            sourceIndex += value;
            [value, position] = decodeVlq(segments, position);
            sourceLine += value;
            [value, position] = decodeVlq(segments, position);
            sourceColumn += value;
            // Skip the optional name index.
            if (position < segmentEnd) {
              [, position] = decodeVlq(segments, position);
            }
            if (i === line - 1 && generatedColumn <= column) {
              best = {
                sourceIndex,
                line: sourceLine + 1,
                column: sourceColumn,
              };
            }
          } else if (i === line - 1 && generatedColumn <= column) {
            // A single-field segment marks generated code with no original
            // source; if it is the greatest lower bound, there is no answer.
            best = null;
          }
        }
        position = segmentEnd + 1;
      }
    }

    if (!best || !map.sources[best.sourceIndex]) {
      return null;
    }
    let source = map.sources[best.sourceIndex];
    // Match the source-map library's util.join: a source that is already
    // absolute (has a scheme like webpack:// or starts with "/") is used
    // as-is; only relative sources are joined onto the root.
    if (
      map.sourceRoot &&
      !/^[A-Za-z][A-Za-z0-9+.-]*:/.test(source) &&
      !source.startsWith("/")
    ) {
      source = map.sourceRoot.replace(/\/?$/, "/") + source;
    }
    return { source, line: best.line, column: best.column };
  }

  // ---------------------------------------------------------------------------
  // React component source resolution
  //
  // Ported from click-to-component: find the React fiber for an element, force
  // the owning component function to throw, diff its stack against a control
  // stack to find the component's call site in the bundle, then map that
  // position back through the dev server's source map.
  // ---------------------------------------------------------------------------

  /**
   * @typedef {Function & { render?: Function }} ComponentFunction
   * @typedef {{ _debugOwner?: { type?: ComponentFunction } | null }} ReactFiber
   * @typedef {{ url: string | undefined, line: number, column: number }} StackFrame
   */

  /**
   * @param {Element} element
   * @returns {ReactFiber | undefined}
   */
  function getReactInstanceForElement(element) {
    const record = /** @type {Record<string, unknown>} */ (
      /** @type {unknown} */ (element)
    );
    for (const key in record) {
      if (key.startsWith("__reactFiber")) {
        return /** @type {ReactFiber} */ (record[key]);
      }
    }
    return undefined;
  }

  // V8's CallSite objects and Error.prepareStackTrace are not in the DOM lib.
  /**
   * @typedef {{
   *   getScriptNameOrSourceURL(): string | undefined,
   *   getLineNumber(): number,
   *   getColumnNumber(): number,
   * }} V8CallSite
   */
  const V8Error = /**
   * @type {ErrorConstructor & {
   *   prepareStackTrace?: (error: Error, stack: V8CallSite[]) => unknown,
   * }}
   */ (Error);

  /**
   * @param {Function} fn
   * @param {boolean} construct
   * @returns {StackFrame | undefined}
   */
  function getComponentInfoFromStack(fn, construct) {
    // The structured stack frames below are V8-only.
    if (!(/** @type {{ chrome?: unknown }} */ (window).chrome)) {
      return undefined;
    }

    const oldPrepareStackTrace = V8Error.prepareStackTrace;
    V8Error.prepareStackTrace = (_, stack) =>
      stack.map((frame) => ({
        url: frame.getScriptNameOrSourceURL(),
        line: frame.getLineNumber(),
        column: frame.getColumnNumber(),
      }));

    // Adapted from React DevTools' DevToolsComponentStackFrame.
    // While the prepareStackTrace override above is installed, `.stack` holds
    // the structured frames it returned instead of the usual string.
    /** @type {{ stack?: StackFrame[] } | undefined} */
    let controlError;
    /** @type {{ stack?: StackFrame[] } | undefined} */
    let sampleError;
    let controlStack;
    let sampleStack;
    try {
      try {
        if (construct) {
          const Fake = function () {
            throw Error();
          };
          Object.defineProperty(Fake.prototype, "props", {
            set: function () {
              // A throwing setter instead of frozen or non-writable props
              // because that won't throw in a non-strict mode function.
              throw Error();
            },
          });
          if (typeof Reflect === "object" && Reflect.construct) {
            try {
              Reflect.construct(Fake, []);
            } catch (x) {
              controlError = /** @type {{ stack?: StackFrame[] }} */ (x);
            }
            Reflect.construct(fn, [], Fake);
          } else {
            try {
              Fake.call(undefined);
            } catch (x) {
              controlError = /** @type {{ stack?: StackFrame[] }} */ (x);
            }
            fn.call(Fake.prototype);
          }
        } else {
          try {
            const fake = function () {
              throw Error();
            };
            fake();
          } catch (x) {
            controlError = /** @type {{ stack?: StackFrame[] }} */ (x);
          }
          const maybePromise = fn();
          // An async component returns a promise; silence its rejection.
          if (maybePromise && typeof maybePromise.catch === "function") {
            maybePromise.catch(() => {});
          }
        }
      } catch (x) {
        sampleError = /** @type {{ stack?: StackFrame[] }} */ (x);
      }
    } finally {
      if (controlError) {
        controlStack = controlError.stack;
      }
      if (sampleError) {
        sampleStack = sampleError.stack;
      }
      V8Error.prepareStackTrace = oldPrepareStackTrace;
    }
    if (
      sampleStack &&
      controlStack &&
      sampleStack.length >= controlStack.length
    ) {
      // The first frame past the shared control frames is the component call.
      return sampleStack[sampleStack.length - controlStack.length];
    }
    return undefined;
  }

  /**
   * @typedef {{
   *   componentName?: string,
   *   fileName?: string,
   *   lineNumber?: number,
   *   columnNumber?: number,
   * }} ComponentSource
   */

  // Returns one of {}, { componentName }, or
  // { componentName, fileName, lineNumber, columnNumber }.
  /**
   * @param {ReactFiber} instance
   * @returns {Promise<ComponentSource>}
   */
  async function getSourceForInstance(instance) {
    if (!instance?._debugOwner?.type) {
      return {};
    }

    let fnOrClass = instance._debugOwner.type;
    // Host fibers carry a plain tag-name string; only component functions or
    // forward-ref objects can yield a stack.
    if (typeof fnOrClass !== "function" && typeof fnOrClass !== "object") {
      return {};
    }
    // A forward ref wraps the actual component in `render`.
    if ("render" in fnOrClass && fnOrClass.render) {
      fnOrClass = fnOrClass.render;
    }

    try {
      let stackTop = getComponentInfoFromStack(fnOrClass, false);
      if (!stackTop) {
        stackTop = getComponentInfoFromStack(fnOrClass, true);
        if (!stackTop) {
          throw new Error("Could not capture component stack");
        }
      }

      const { url, line, column } = stackTop;
      if (!url) {
        throw new Error("Top of stack does not contain valid url");
      }

      // Bundlers that eval modules produce fake sourceURLs
      // (webpack-internal://...) that can't be fetched; degrade below.
      const response = await fetch(url);
      const srcFile = await response.text();

      const sourceMapMatch = srcFile.match(
        /^\/\/[@#]\s*sourceMappingURL=(\S*?)\s*$/m
      );
      if (!sourceMapMatch || !sourceMapMatch[1]) {
        throw new Error("Minified source file does not contain sourcemap URL");
      }

      const sourceMapUrl = new URL(sourceMapMatch[1], url).toString();
      const sourceMapResponse = await fetch(sourceMapUrl);
      if (!sourceMapResponse.ok) {
        throw new Error("Failed to fetch sourcemap");
      }
      // Strip the ")]}'" XSSI-protection prefix some servers prepend
      // (parseSourceMapInput in the source-map library did the same).
      const sourceMapText = await sourceMapResponse.text();
      const sourceMap = JSON.parse(
        sourceMapText.replace(/^\)\]\}'[^\n]*\n/, "")
      );

      const originalPosition = originalPositionFor(sourceMap, line, column);
      if (!originalPosition) {
        throw new Error("Failed to get original position from source map");
      }

      return {
        componentName: fnOrClass.name,
        fileName: originalPosition.source,
        lineNumber: originalPosition.line,
        columnNumber: originalPosition.column,
      };
    } catch {
      if ("name" in fnOrClass) {
        return { componentName: fnOrClass.name };
      }
      return {};
    }
  }

  /**
   * A source counts as resolved only when it names the component or its file;
   * an anonymous component can yield `{ componentName: "" }`.
   * @param {ComponentSource | undefined} source
   * @returns {source is ComponentSource}
   */
  function hasSourceInfo(source) {
    return Boolean(source && (source.componentName || source.fileName));
  }

  /**
   * @param {Element} element
   * @returns {Promise<ComponentSource | undefined>}
   */
  async function getSourceForElement(element) {
    const instance = getReactInstanceForElement(element);
    if (!instance) {
      return undefined;
    }

    const source = await getSourceForInstance(instance);
    if (hasSourceInfo(source)) {
      return source;
    }

    // Walk up until an ancestor resolves.
    let parent = element.parentElement;
    while (parent) {
      const parentInstance = getReactInstanceForElement(parent);
      const parentSource = await getSourceForInstance(parentInstance ?? {});
      if (hasSourceInfo(parentSource)) {
        return parentSource;
      }
      parent = parent.parentElement;
    }
    return undefined;
  }

  // ---------------------------------------------------------------------------
  // UI
  // ---------------------------------------------------------------------------

  const SVG_NS = "http://www.w3.org/2000/svg";
  // @heroicons 24/solid CursorArrowRippleIcon and XMarkIcon (MIT).
  const CURSOR_ICON_PATH =
    "M17.303 5.197A7.5 7.5 0 0 0 6.697 15.803a.75.75 0 0 1-1.061 1.061A9 9 0 1 1 21 10.5a.75.75 0 0 1-1.5 0c0-1.92-.732-3.839-2.197-5.303Zm-2.121 2.121a4.5 4.5 0 0 0-6.364 6.364.75.75 0 1 1-1.06 1.06A6 6 0 1 1 18 10.5a.75.75 0 0 1-1.5 0c0-1.153-.44-2.303-1.318-3.182Zm-3.634 1.314a.75.75 0 0 1 .82.311l5.228 7.917a.75.75 0 0 1-.777 1.148l-2.097-.43 1.045 3.9a.75.75 0 0 1-1.45.388l-1.044-3.899-1.601 1.42a.75.75 0 0 1-1.247-.606l.569-9.47a.75.75 0 0 1 .554-.68Z";
  const XMARK_ICON_PATH =
    "M5.47 5.47a.75.75 0 0 1 1.06 0L12 10.94l5.47-5.47a.75.75 0 1 1 1.06 1.06L13.06 12l5.47 5.47a.75.75 0 1 1-1.06 1.06L12 13.06l-5.47 5.47a.75.75 0 0 1-1.06-1.06L10.94 12 5.47 6.53a.75.75 0 0 1 0-1.06Z";

  /** @param {string} pathData */
  function icon(pathData) {
    const svg = document.createElementNS(SVG_NS, "svg");
    svg.setAttribute("viewBox", "0 0 24 24");
    svg.setAttribute("fill", "currentColor");
    svg.setAttribute("aria-hidden", "true");
    svg.style.width = "16px";
    svg.style.height = "16px";
    const path = document.createElementNS(SVG_NS, "path");
    path.setAttribute("fill-rule", "evenodd");
    path.setAttribute("clip-rule", "evenodd");
    path.setAttribute("d", pathData);
    svg.appendChild(path);
    return svg;
  }

  // Styles are assigned through the CSSOM, which no style-src policy governs,
  // so the toolbar renders identically under a strict page CSP.
  /**
   * @param {ElementCSSInlineStyle} element
   * @param {Partial<CSSStyleDeclaration>} styles
   */
  function applyStyles(element, styles) {
    Object.assign(element.style, styles);
  }

  const BUTTON_STYLE = {
    all: "unset",
    backgroundColor: "#ffffff",
    color: "#000000",
    border: "none",
    padding: "6px 10px",
    cursor: "pointer",
    fontSize: "13px",
    fontFamily: "system-ui, -apple-system, sans-serif",
    fontWeight: "600",
    display: "flex",
    alignItems: "center",
    gap: "4px",
    transition: "background-color 0.2s ease",
    position: "relative",
    outline: "none",
  };

  class BrowserPreviewUi {
    /**
     * @param {{
     *   onSelectElement: (element: Element) => void,
     *   onSendConsoleOutput: () => void,
     * }} callbacks
     */
    constructor({ onSelectElement, onSendConsoleOutput }) {
      this.onSelectElement = onSelectElement;
      this.onSendConsoleOutput = onSendConsoleOutput;

      this.isSelecting = false;
      this.errorCount = 0;
      /** @type {ReturnType<typeof setTimeout> | undefined} */
      this.notificationTimer = undefined;
      /** @type {Animation | null} */
      this.fadeAnimation = null;

      this.container = document.createElement("div");
      this.container.className = "toolbar-container";
      document.body.appendChild(this.container);

      this.toolbar = document.createElement("div");
      this.selectButton = document.createElement("button");
      this.errorsButton = document.createElement("button");
      this.notification = document.createElement("div");
      this.notificationBody = document.createElement("div");
      this.notificationText = document.createElement("span");
      this.notificationMessage = document.createTextNode("");

      this.buildToolbar();
      this.buildNotification();
      this.selector = new ElementSelector({
        onSelect: (element) => this.onSelectElement(element),
        setIsSelecting: (selecting) => this.setSelecting(selecting),
      });
    }

    buildToolbar() {
      applyStyles(this.toolbar, {
        all: "unset",
        position: "fixed",
        bottom: "20px",
        right: "20px",
        backgroundColor: "#ffffff",
        borderRadius: "4px",
        boxShadow: "0 4px 12px rgba(0, 0, 0, 0.2)",
        zIndex: String(MAX_Z_INDEX),
        display: "flex",
        alignItems: "center",
      });

      applyStyles(this.selectButton, BUTTON_STYLE);
      this.selectButton.addEventListener("click", () => {
        this.setSelecting(!this.isSelecting);
      });
      this.selectButton.addEventListener("mouseenter", () => {
        if (!this.isSelecting) {
          this.selectButton.style.backgroundColor = "#f5f5f5";
        }
      });
      this.selectButton.addEventListener("mouseleave", () => {
        if (!this.isSelecting) {
          this.selectButton.style.backgroundColor = "#ffffff";
        }
      });

      applyStyles(this.errorsButton, BUTTON_STYLE);
      this.errorsButton.style.borderTopRightRadius = "4px";
      this.errorsButton.style.borderBottomRightRadius = "4px";
      this.errorsButton.addEventListener("click", () => {
        if (this.errorCount === 0) {
          return;
        }
        this.onSendConsoleOutput();
      });
      this.errorsButton.addEventListener("mouseenter", () => {
        if (this.errorCount > 0) {
          this.errorsButton.style.backgroundColor = "#f5f5f5";
        }
      });
      this.errorsButton.addEventListener("mouseleave", () => {
        if (this.errorCount > 0) {
          this.errorsButton.style.backgroundColor = "#ffffff";
        }
      });

      this.toolbar.appendChild(this.selectButton);
      this.toolbar.appendChild(this.errorsButton);
      this.container.appendChild(this.toolbar);
      this.renderToolbar();
    }

    renderToolbar() {
      this.selectButton.replaceChildren();
      const label = document.createElement("span");
      applyStyles(label, {
        display: "flex",
        alignItems: "center",
        gap: "4px",
      });
      label.append(
        this.isSelecting ? "Stop selecting" : "Send element",
        icon(this.isSelecting ? XMARK_ICON_PATH : CURSOR_ICON_PATH)
      );
      this.selectButton.appendChild(label);
      applyStyles(this.selectButton, {
        borderTopLeftRadius: "4px",
        borderBottomLeftRadius: "4px",
        borderTopRightRadius: this.isSelecting ? "4px" : "0",
        borderBottomRightRadius: this.isSelecting ? "4px" : "0",
        backgroundColor: this.isSelecting ? "#b7d0fc" : "#ffffff",
        color: this.isSelecting ? "#1a73e8" : "#000000",
      });

      this.errorsButton.textContent = `Send console errors (${this.errorCount})`;
      this.errorsButton.disabled = this.errorCount === 0;
      applyStyles(this.errorsButton, {
        backgroundColor: this.errorCount === 0 ? "#f0f0f0" : "#ffffff",
        color: this.errorCount === 0 ? "#999999" : "#000000",
        cursor: this.errorCount === 0 ? "default" : "pointer",
      });
      this.errorsButton.style.display = this.isSelecting ? "none" : "flex";
    }

    /** @param {boolean} selecting */
    setSelecting(selecting) {
      if (this.isSelecting === selecting) {
        return;
      }
      this.isSelecting = selecting;
      this.selector.setActive(selecting);
      this.renderToolbar();
    }

    /** @param {number} count */
    setErrorCount(count) {
      this.errorCount = count;
      this.renderToolbar();
    }

    buildNotification() {
      applyStyles(this.notification, {
        all: "unset",
        position: "fixed",
        bottom: "20px",
        right: "20px",
        borderRadius: "4px",
        zIndex: String(MAX_Z_INDEX),
        display: "none",
        alignItems: "center",
        pointerEvents: "auto",
      });

      applyStyles(this.notificationBody, {
        backgroundColor: "#3180E8",
        color: "#ffffff",
        padding: "6px 10px",
        borderRadius: "4px",
        fontSize: "13px",
        fontWeight: "600",
        border: "1px solid #1a73e8",
      });

      applyStyles(this.notificationText, {
        display: "flex",
        alignItems: "center",
        gap: "8px",
      });

      const close = icon(XMARK_ICON_PATH);
      close.style.cursor = "pointer";
      close.addEventListener("click", (event) => {
        event.preventDefault();
        event.stopPropagation();
        this.hideNotification();
      });

      this.notificationText.append(this.notificationMessage, close);
      this.notificationBody.appendChild(this.notificationText);
      this.notification.appendChild(this.notificationBody);
      this.container.appendChild(this.notification);
    }

    // Shows the notification in place of the toolbar for three seconds.
    /** @param {string} message */
    showNotification(message) {
      // Cancel any in-flight fade-out so its onfinish can't hide this
      // notification (cancel fires oncancel, not onfinish).
      if (this.fadeAnimation) {
        this.fadeAnimation.cancel();
        this.fadeAnimation = null;
      }
      this.notificationMessage.textContent = message;
      this.notification.style.display = "flex";
      this.toolbar.style.display = "none";
      // element.animate keeps the animation out of CSP-governed stylesheets.
      this.notification.animate(
        [
          { transform: "translateY(10px)", opacity: 0 },
          { transform: "translateY(0)", opacity: 1 },
        ],
        { duration: 250, easing: "ease-out" }
      );
      clearTimeout(this.notificationTimer);
      this.notificationTimer = setTimeout(() => this.hideNotification(), 3000);
    }

    hideNotification() {
      clearTimeout(this.notificationTimer);
      // Cancel any in-flight fade so overlapping hides can't leave an
      // untracked fade whose onfinish hides a later notification.
      if (this.fadeAnimation) {
        this.fadeAnimation.cancel();
      }
      const fade = this.notification.animate(
        [
          { transform: "translateY(0)", opacity: 1 },
          { transform: "translateY(-10px)", opacity: 0 },
        ],
        { duration: 250, easing: "ease-out" }
      );
      fade.onfinish = () => {
        this.notification.style.display = "none";
        this.toolbar.style.display = "flex";
        this.fadeAnimation = null;
      };
      this.fadeAnimation = fade;
    }
  }

  class ElementSelector {
    /**
     * @param {{
     *   onSelect: (element: Element) => void,
     *   setIsSelecting: (selecting: boolean) => void,
     * }} callbacks
     */
    constructor({ onSelect, setIsSelecting }) {
      this.onSelect = onSelect;
      this.setIsSelecting = setIsSelecting;
      /** @type {HTMLDivElement | null} */
      this.overlay = null;
      /** @type {HTMLDivElement | null} */
      this.label = null;
      /** @type {Element | null} */
      this.hoveredElement = null;

      /** @param {MouseEvent} event */
      this.handleMouseOver = (event) => {
        const element = /** @type {HTMLElement} */ (event.target);
        if (element === this.overlay || element.closest(".toolbar-container")) {
          return;
        }
        event.stopPropagation();
        this.hoveredElement = element;
        this.highlight(element);
        getSourceForElement(element).then((result) => {
          element.dataset.componentName =
            result?.componentName || `<${element.nodeName.toLowerCase()} />`;
          // Resolution is async; only re-highlight (to show the resolved
          // name) if the pointer is still on this element.
          if (this.hoveredElement === element) {
            this.highlight(element);
          }
        });
      };

      /** @param {MouseEvent} event */
      this.handleClick = (event) => {
        const target = /** @type {Element | null} */ (event.target);
        if (
          !target ||
          target === this.overlay ||
          target.closest(".toolbar-container")
        ) {
          return;
        }
        event.preventDefault();
        event.stopPropagation();
        // A modifier keeps selection mode active for capturing several
        // elements in a row.
        if (!(event.ctrlKey || event.metaKey || event.shiftKey)) {
          this.setIsSelecting(false);
        }
        this.onSelect(target);
      };

      /** @param {KeyboardEvent} event */
      this.handleKeyDown = (event) => {
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          this.setIsSelecting(false);
        }
      };
    }

    /** @param {boolean} active */
    setActive(active) {
      if (active) {
        document.addEventListener("mouseover", this.handleMouseOver);
        // Capture phase, to intercept before the page's own handlers.
        document.addEventListener("click", this.handleClick, true);
        document.addEventListener("keydown", this.handleKeyDown);

        this.overlay = document.createElement("div");
        applyStyles(this.overlay, {
          all: "unset",
          position: "fixed",
          pointerEvents: "none",
          zIndex: String(MAX_Z_INDEX - 1),
          transition: "all 0.1s ease-in-out",
          display: "none",
          border: "2px solid #1a73e8",
          backgroundColor: "#e8f0fe44",
        });

        this.label = document.createElement("div");
        this.label.className = "element-name-label";
        applyStyles(this.label, {
          all: "unset",
          position: "absolute",
          top: "-30px",
          left: "-1px",
          color: "white",
          backgroundColor: "#1a73e8",
          padding: "4px 10px",
          borderTopLeftRadius: "4px",
          borderTopRightRadius: "4px",
          fontSize: "14px",
          fontWeight: "500",
          fontFamily: "sans-serif",
          pointerEvents: "none",
          boxShadow: "0 1px 3px rgba(0,0,0,0.2)",
          whiteSpace: "nowrap",
          textOverflow: "ellipsis",
          overflow: "hidden",
        });
        this.overlay.appendChild(this.label);
        document.body.appendChild(this.overlay);
      } else {
        document.removeEventListener("mouseover", this.handleMouseOver);
        document.removeEventListener("click", this.handleClick, true);
        document.removeEventListener("keydown", this.handleKeyDown);
        this.overlay?.remove();
        this.overlay = null;
        this.label = null;
      }
    }

    /** @param {HTMLElement} element */
    highlight(element) {
      if (!this.overlay || !this.label) {
        return;
      }
      // -2 to account for the border.
      const rect = element.getBoundingClientRect();
      applyStyles(this.overlay, {
        display: "block",
        top: `${rect.top - 2}px`,
        left: `${rect.left - 2}px`,
        width: `${rect.width}px`,
        height: `${rect.height}px`,
      });
      this.label.textContent =
        element.dataset.componentName || element.nodeName.toLowerCase();
    }
  }

  // ---------------------------------------------------------------------------
  // Captures
  // ---------------------------------------------------------------------------

  /** @typedef {{ type: string, datetime: string, value: unknown[] }} ConsoleEntry */

  class BrowserPreview {
    constructor() {
      /** @type {ConsoleEntry[]} */
      this.consoleOutput = [];
      this.consoleSendInFlight = false;
      this.setupConsoleCapture();
      this.ui = new BrowserPreviewUi({
        onSelectElement: (element) => this.captureElement(element),
        onSendConsoleOutput: () => this.captureConsoleOutput(),
      });
    }

    setupConsoleCapture() {
      const originalError = console.error;
      console.error = (...args) => {
        this.consoleOutput.push({
          type: "error",
          datetime: new Date().toLocaleString(),
          value: args,
        });
        if (this.consoleOutput.length > MAX_CONSOLE_ENTRIES) {
          this.consoleOutput.splice(
            0,
            this.consoleOutput.length - MAX_CONSOLE_ENTRIES
          );
        }
        this.ui?.setErrorCount(this.consoleOutput.length);
        originalError.apply(console, args);
      };
    }

    /** @param {Element} element */
    async captureElement(element) {
      if (!element) {
        return;
      }
      try {
        /** @type {DomElementScopeItem} */
        const domElement = {
          tagName: element.localName,
          id: element.id,
          outerHtml: element.outerHTML,
        };

        const source = await getSourceForElement(element);
        if (source?.componentName) {
          domElement.reactComponentName = source.componentName;
          if (source.fileName) {
            domElement.fileLineRange = {
              absoluteUri: source.fileName,
              startLine: source.lineNumber,
            };
          }
        }

        await rpc("SendDOMElement", encodeDomElement(domElement));
        this.ui.showNotification("Element was added to Devin");
      } catch (error) {
        console.warn("Error capturing element:", error);
      }
    }

    async captureConsoleOutput() {
      if (this.consoleOutput.length === 0 || this.consoleSendInFlight) {
        return;
      }
      this.consoleSendInFlight = true;
      // Send every buffered entry so the badge count matches what a single
      // send delivers; only entries logged while the RPC is in flight are
      // kept for the next send. The snapshot is held by identity because the
      // capture-time cap can trim the buffer (shifting indices) mid-send.
      const sentEntries = new Set(this.consoleOutput);
      try {
        const lines = this.consoleOutput.map((entry) => ({
          timestampStr: entry.datetime,
          type: entry.type.toUpperCase(),
          output: entry.value
            .map((value) => {
              if (typeof value === "object") {
                try {
                  return JSON.stringify(value);
                } catch {
                  return String(value);
                }
              }
              return String(value);
            })
            .join(" "),
        }));

        await rpc(
          "SendConsoleOutput",
          encodeConsoleLog({
            lines,
            serverAddress: window.location.host,
          })
        );

        // Remove exactly the entries that were sent, keeping any that arrived
        // during the in-flight RPC.
        this.consoleOutput = this.consoleOutput.filter(
          (entry) => !sentEntries.has(entry)
        );
        this.ui.setErrorCount(this.consoleOutput.length);
        this.ui.showNotification("Errors were added to Devin");
      } catch (error) {
        console.warn("Error capturing console output:", error);
      } finally {
        this.consoleSendInFlight = false;
      }
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => new BrowserPreview());
  } else {
    new BrowserPreview();
  }
})();
