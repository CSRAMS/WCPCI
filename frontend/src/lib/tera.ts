export const variable = (expression: string, debugEval?: string) =>
    import.meta.env.DEV ? (debugEval ?? expression) : `{{ ${expression} }}`;
export const tag = (expression: string, whitespace?: boolean) =>
    `{%${whitespace ? "" : "-"} ${expression} ${whitespace ? "" : "-"}%}`;
export const teraIf = (
    condition: string,
    t: string,
    f?: string,
    debugEval?: string,
    whitespace?: boolean
) =>
    import.meta.env.DEV
        ? debugEval
        : `${tag(`if ${condition}`, whitespace)}${t}${f ? `${tag("else")}${f}` : ""}${tag("endif", whitespace)}`;

export const themeClass = (light: string, dark: string, system?: string) => {
    return import.meta.env.DEV
        ? (system ?? dark)
        : `${tag("if scheme == 'Dark'", true)}${dark}${tag("elif scheme == 'Light'")}${light}${tag("else")}${system ?? dark}${tag("endif", true)}`;
};
