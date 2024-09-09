import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { go } from "@codemirror/lang-go";
import { java } from "@codemirror/lang-java";
import { php } from "@codemirror/lang-php";
import { cpp } from "@codemirror/lang-cpp";
import { rust } from "@codemirror/lang-rust";
import { julia } from "@plutojl/lang-julia";
import { clojure } from "@nextjournal/lang-clojure";
import { csharp } from "@replit/codemirror-lang-csharp";
import { zig } from "codemirror-lang-zig";
import { haskell } from "@flok-editor/lang-haskell";
import { r } from "codemirror-lang-r";
import { type Extension } from "@codemirror/state";

export default {
    javascript,
    python,
    go,
    java,
    php,
    cpp,
    rust,
    julia,
    clojure,
    csharp,
    zig,
    haskell,
    r
} as Record<string, () => Extension>;
