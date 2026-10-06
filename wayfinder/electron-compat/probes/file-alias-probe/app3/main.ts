import dep from "dep"; import {app} from "electron"; console.log(JSON.stringify({appType: typeof app, dep:dep.fromDep}));
