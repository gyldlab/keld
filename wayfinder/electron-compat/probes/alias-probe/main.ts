import { app } from "electron";
import dep from "dep";
console.log("app-code electron ->", app.name);
console.log("node_modules dep electron ->", dep.fromDep);
