import {app} from "electron"; import {app as m} from "electron/main"; import dep from "dep"; console.log(JSON.stringify({app:app.name, main:m.name, dep:dep.fromDep}));
