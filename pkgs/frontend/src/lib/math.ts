import "katex/dist/katex.css";
import katex from "katex";

export default (selectorPrefix?: string) => {
    document
        .querySelectorAll(`${selectorPrefix ?? "#rendered-md"} code.math-inline`)
        .forEach((block) => {
            katex.render(block.textContent ?? "", block as HTMLElement, { throwOnError: false });
        });
    document
        .querySelectorAll(`${selectorPrefix ?? "#rendered-md"} pre code.math-display`)
        .forEach((block) => {
            katex.render(block.textContent ?? "", block as HTMLElement, {
                throwOnError: false,
                displayMode: true
            });
        });
};
