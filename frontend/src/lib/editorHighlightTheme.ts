import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";

const v = (va: string) => `var(--${va})`;

export default syntaxHighlighting(
    HighlightStyle.define([
        {
            tag: t.keyword,
            color: v("primary-700")
        },
        {
            tag: [
                t.name,
                t.deleted,
                t.character,
                t.propertyName,
                t.macroName,
                t.operator,
                t.operatorKeyword
            ],
            color: v("text-800")
        },
        { tag: [t.function(t.variableName), t.labelName], color: v("primary-900") },
        { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: v("accent-500") },
        {
            tag: [
                t.typeName,
                t.className,
                t.number,
                t.changed,
                t.annotation,
                t.modifier,
                t.self,
                t.namespace
            ],
            color: v("accent-600")
        },
        {
            tag: [t.url, t.escape, t.regexp, t.link, t.special(t.string)],
            color: v("accent-600")
        },
        { tag: [t.meta, t.comment], color: v("text-500") },
        { tag: t.strikethrough, textDecoration: "line-through" },
        { tag: t.link, textDecoration: "underline" },
        { tag: t.heading, fontWeight: "bold" },
        { tag: [t.atom, t.bool, t.special(t.variableName)], color: v("text-700") },
        { tag: [t.processingInstruction, t.string, t.inserted], color: v("accent-500") },
        { tag: t.invalid, color: "red" }
    ])
);
