import "highlight.js/styles/default.css";
import "@/styles/highlight-theme.scss";
import hljs from "highlight.js";
import text from "highlight.js/lib/languages/plaintext";

hljs.registerLanguage("example", text);

const makePreCode = (text: string): HTMLPreElement => {
    const pre = document.createElement("pre");
    const code = document.createElement("code");
    code.classList.add("language-example");
    code.textContent = text;
    pre.appendChild(code);
    return pre;
};

export default (
    selectorPrefix?: string,
    onRunExample?: (input: string) => void,
    exampleButtonTemplate?: HTMLButtonElement,
    onFirstExample?: (input: string) => void
) => {
    document
        .querySelectorAll(
            `${selectorPrefix !== undefined ? selectorPrefix + " " : ""}pre code:not(.language-math):not(language-example)`
        )
        .forEach((block) => {
            hljs.highlightElement(block as HTMLElement);
        });

    if (exampleButtonTemplate) {
        let first = false;

        document
            .querySelectorAll(
                `${selectorPrefix !== undefined ? selectorPrefix + " " : ""}pre code.language-example`
            )
            .forEach((block) => {
                const wrapperElem = document.createElement("div");
                wrapperElem.classList.add("relative");
                const clonedButton = exampleButtonTemplate.cloneNode(true) as HTMLButtonElement;
                clonedButton.removeAttribute("id");
                clonedButton.classList.remove("hidden");
                clonedButton.onclick = () => {
                    onRunExample?.(block.textContent ?? "");
                };
                wrapperElem.appendChild(clonedButton);
                const newBlock = makePreCode(block.textContent ?? "");
                wrapperElem.appendChild(newBlock);
                block.parentElement!.replaceWith(wrapperElem);
                hljs.highlightElement(newBlock.childNodes[0] as HTMLElement);
                if (!first) {
                    first = true;
                    onFirstExample?.(block.textContent ?? "");
                }
            });
    }
};
