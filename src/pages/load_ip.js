const config = document.getElementById("js-params");
const container = document.getElementById("unfilled-ip-container");
const code = document.getElementById("unfilled-ip-code");
fetch(config.dataset.endpoint)
  .then((req) => req.text())
  .then((resp) => {
    code.innerText = resp.trim();
    container.hidden = false;
    if (config.dataset.setTitle) {
      document.title = `Your IP is ${resp}`;
    }
  })
  .catch((failed) => {
    console.log("Request failed:" + failed);
  });
