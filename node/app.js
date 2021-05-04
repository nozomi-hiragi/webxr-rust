import * as wasm from "webxr-rust";

var xrApp = new wasm.XrApp();
xrApp.init()
    .then(res => {
        if (res) {
            console.log('init ok');
            xrApp.start();
        }
        else {
            console.log('init failed');
        }
    });
