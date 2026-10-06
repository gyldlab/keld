import { app } from "electron";
import dep from "dep";
import { fromEsmDep } from "esmdep";
console.log(JSON.stringify({ app: app.name, cjsDep: dep.fromDep, esmDep: fromEsmDep }));
