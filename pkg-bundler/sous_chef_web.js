/* @ts-self-types="./sous_chef_web.d.ts" */
import * as wasm from "./sous_chef_web_bg.wasm";
import { __wbg_set_wasm } from "./sous_chef_web_bg.js";

__wbg_set_wasm(wasm);
wasm.__wbindgen_start();
export {
    analyze_vref
} from "./sous_chef_web_bg.js";
