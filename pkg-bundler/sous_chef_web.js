/* @ts-self-types="./sous_chef_web.d.ts" */
import * as wasm from "./sous_chef_web_bg.wasm";
import { __wbg_set_wasm } from "./sous_chef_web_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    Galley, analyze_vref, census, rule_catalog
} from "./sous_chef_web_bg.js";
