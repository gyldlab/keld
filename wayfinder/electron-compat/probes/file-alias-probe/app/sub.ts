import {app} from "electron"; import {app as m} from "electron/main"; console.log(JSON.stringify({app:app.name, main:m.name}));
