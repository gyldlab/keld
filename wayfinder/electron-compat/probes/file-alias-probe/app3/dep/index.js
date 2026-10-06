let r; try { r = Object.keys(require("electron")).join(","); } catch(e) { r = "ERR: "+e.message.slice(0,120); } module.exports={fromDep:r};
