import editorLanguages from "@/lib/editorLanguages";
import baseExt from "@/lib/editorFeatures";
import { EditorView } from "@codemirror/view";
import { Compartment, EditorState } from "@codemirror/state";
import editorHighlightTheme from "./editorHighlightTheme";

type LanguageDisplayInfo = {
    name: string;
    deviconIcon?: string;
    monacoContribution: string;
    defaultCode: string;
};

export type CodeInfo = {
    [lang: string]: LanguageDisplayInfo;
};

const getIconName = (key: string, lang: LanguageDisplayInfo) => lang.deviconIcon ?? key;
const makeIconClass = (icon: string) => `devicon-${icon}-plain`;

export default (
    codeInfo: CodeInfo,
    defaultLanguage: string,
    contestId: string,
    problemId: string,
    languageDropdown: HTMLSelectElement,
    colorScheme: string,
    editorElem: HTMLElement,
    languageIcon: HTMLSpanElement,
    saveIndicator: HTMLElement,
    resetButton: HTMLButtonElement,
    mostRecentCode: [string, string] | null
) => {
    let editor: EditorView | null = null;
    let currentLanguage = defaultLanguage;

    const languageCompartment = new Compartment();

    const setLanguage = (lang: LanguageDisplayInfo) => {
        if (editor) {
            editor.dispatch({
                effects: languageCompartment.reconfigure(editorLanguages[lang.monacoContribution]())
            });
        }
    };

    const setLanguageIcon = (name: string) => {
        const currentClass = languageIcon.className
            .split(" ")
            .filter((c) => !c.startsWith("devicon-"));
        languageIcon.className = [...currentClass, makeIconClass(name)].join(" ");
    };

    const setEditorContent = (content: string) => {
        if (editor) {
            editor.dispatch({
                changes: { from: 0, to: editor.state.doc.length, insert: content }
            });
        }
    };

    languageDropdown.onchange = (e) => {
        const lang = (e.target as HTMLSelectElement).value;
        const langInfo = codeInfo[lang];
        if (langInfo) {
            currentLanguage = lang;
            setLanguageIcon(getIconName(lang, langInfo));
            if (editor) {
                const storedCode = JSON.parse(
                    window.localStorage.getItem(
                        `contest-${contestId}-problem-${problemId}-${lang}-code`
                    ) ?? "null"
                );
                setEditorContent(storedCode ?? langInfo.defaultCode);
                setLanguage(langInfo);
                window.localStorage.setItem(
                    `contest-${contestId}-problem-${problemId}-code`,
                    JSON.stringify([storedCode, lang])
                );
            }
        }
    };

    const [storedCode, storedLang] = JSON.parse(
        window.localStorage.getItem(`contest-${contestId}-problem-${problemId}-code`) ??
            "[null, null]"
    );

    currentLanguage = Object.keys(codeInfo).includes(storedLang ?? "")
        ? storedLang
        : mostRecentCode && Object.keys(codeInfo).includes(mostRecentCode[1])
          ? mostRecentCode[1]
          : defaultLanguage;

    const langInfo = codeInfo[currentLanguage];

    languageDropdown.value = currentLanguage;
    setLanguageIcon(getIconName(currentLanguage, langInfo));
    setTimeout(() => languageIcon.classList.remove("opacity-0"), 300);

    const saveChanges = () => {
        if (!editor) return;
        const text = editor.state.doc.toString();
        window.localStorage.setItem(
            `contest-${contestId}-problem-${problemId}-code`,
            JSON.stringify([text, currentLanguage])
        );
        window.localStorage.setItem(
            `contest-${contestId}-problem-${problemId}-${currentLanguage}-code`,
            JSON.stringify(text)
        );
        saveIndicator.dataset.saveState = "saved";
        saveIndicator.ariaLabel = "Changes Saved!";
    };

    const onDocChanged = () => {
        saveIndicator.dataset.saveState = "saving";
        saveIndicator.ariaLabel = "Saving Changes...";
        if (currentTimeout) {
            clearTimeout(currentTimeout);
        }
        currentTimeout = setTimeout(() => {
            if (editor && oldLang === currentLanguage) {
                saveChanges();
            }
        }, 1000) as unknown as number;
        oldLang = currentLanguage!;
    };

    const theme = EditorView.theme({
        "&": { height: "100%", width: "100%", overflow: "auto" }
    });

    const state = EditorState.create({
        extensions: [
            baseExt,
            editorHighlightTheme,
            theme,
            EditorView.updateListener.of((update) => {
                if (!update.docChanged) return;
                onDocChanged();
            }),
            languageCompartment.of(editorLanguages[langInfo.monacoContribution]())
        ],
        doc: storedCode ?? mostRecentCode?.[0] ?? langInfo.defaultCode
    });

    editor = new EditorView({
        state,
        parent: editorElem
    });

    editor;

    let currentTimeout: number | undefined = undefined;
    let oldLang = currentLanguage;

    window.onbeforeunload = () => {
        if (editor && saveIndicator && saveIndicator.dataset.saveState === "saving") {
            saveChanges();
        }
    };

    document.onkeydown = (e) => {
        if (e.ctrlKey && e.key === "s" && editor && saveIndicator) {
            e.preventDefault();
            saveChanges();
            if (currentTimeout) {
                clearTimeout(currentTimeout);
            }
        }
    };

    console.debug("Instantiated Editor");

    resetButton.onclick = () => {
        if (editor) {
            if (
                window.confirm(
                    "Are you sure you want to reset your code? This will erase your changes for the current language."
                )
            ) {
                setEditorContent(codeInfo[currentLanguage].defaultCode);
            }
        }
    };

    return [editor, () => currentLanguage];
};
