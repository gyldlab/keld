// Shared exact negative expectations; no parser/admission implementation.
export const scaffold = "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n";
export const rendererNegativeCases: readonly (readonly [string,string])[] = [
  [
    "<script>console.log(1)</script>",
    "Renderer element outside inert cell"
  ],
  [
    "<script src=\"./app.js\"></script>",
    "Renderer element outside inert cell"
  ],
  [
    "<style>@import \"x.css\";</style>",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<p style=\"background:url(x)\">x</p>",
    "Renderer attribute outside inert cell"
  ],
  [
    "<img src=\"missing.png\">",
    "Renderer element outside inert cell"
  ],
  [
    "<link rel=\"stylesheet\" href=\"x.css\">",
    "Renderer element outside inert cell"
  ],
  [
    "<iframe srcdoc=\"hi\"></iframe>",
    "Renderer element outside inert cell"
  ],
  [
    "<meta http-equiv=\"refresh\" content=\"0;url=x\">",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<base href=\"https://example.invalid\">",
    "Renderer element outside inert cell"
  ],
  [
    "<form action=\"x\"></form>",
    "Renderer element outside inert cell"
  ],
  [
    "<a href=\"x\">x</a>",
    "Renderer element outside inert cell"
  ],
  [
    "<svg><use href=\"x\" /></svg>",
    "Renderer element outside inert cell"
  ],
  [
    "<p onmouseover=\"f()\">x</p>",
    "Renderer attribute outside inert cell"
  ],
  [
    "<div is=\"custom-component\">x</div>",
    "Renderer attribute outside inert cell"
  ],
  [
    "<template><img src=x></template>",
    "Renderer element outside inert cell"
  ],
  [
    "<p>allowed first</p><script>forbidden tail</script>",
    "Renderer element outside inert cell"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #fff;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: url(https://example.invalid);\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        @import \"x\";\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        width: expression(alert(1));\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style onload=\"x()\">\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta http-equiv=\"refresh\" content=\"0;url=x\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <STYLE>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" /><meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Duplicate scaffold meta"
  ],
  [
    "<!--keld-validation-meta-4b73--><p>x</p>",
    "Reserved renderer validation marker"
  ],
  [
    "<!--keld-validation-style-4b73--><p>x</p>",
    "Reserved renderer validation marker"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    \n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        background: #000;\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    &lt;/style&gt;\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer scaffold meta/style differs from exact canonical bytes"
  ],
  [
    "<!doctype html>\n<html lang=\"en\">\n  <head>\n    <meta charset=\"utf-8\" />\n    <title>windows-runnable-hello</title>\n    <style>\n      html {\n        </style><script>1</script><style>\n        color-scheme: dark;\n      }\n      body {\n        font-family: system-ui, sans-serif;\n        margin: 2rem;\n        background: #000;\n        color: #fff;\n      }\n    </style>\n  </head>\n  <body>\n    <h1>windows-runnable-hello</h1>\n    <p>Rendered by the Keld host webview. IPC echo runs in the Bun main process.</p>\n  </body>\n</html>\n",
    "Renderer element outside inert cell"
  ]
];
