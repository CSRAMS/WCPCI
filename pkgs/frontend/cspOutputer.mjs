import {
    inlineScriptHashes,
    inlineStyleHashes,
    extScriptHashes,
    extStyleHashes
} from "./src/generated/sriHashes.mjs";

const output = JSON.stringify({
    inlineScriptHashes,
    inlineStyleHashes,
    extScriptHashes,
    extStyleHashes
});

import fs from "node:fs";

fs.writeFileSync("dist/sriHashes.json", output);
