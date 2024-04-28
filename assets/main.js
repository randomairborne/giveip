const root_dns_name = document.getElementById("root-dns-name").dataset.dnsName;
const scheme = document.getElementById("scheme").dataset.scheme;
const v4_endpoint = `${scheme}://v4.${root_dns_name}/raw`;
const v6_endpoint = `${scheme}://v6.${root_dns_name}/raw`;
const ip4 = document.getElementById("ip4");
const ip6 = document.getElementById("ip6");
const ip4txt = document.getElementById("ip4txt");
const ip6txt = document.getElementById("ip6txt");
if (ip4txt.hidden) {
  fetch(v4_endpoint)
    .then((req) => req.text())
    .then((resp) => {
      ip4.innerText = resp.trim();
      ip4txt.hidden = false;
      document.getElementById("title").innerText = `Your IP is ${resp}`;
    })
    .catch((failed) => {
      console.log("Request failed:" + failed);
    });
}
if (ip6txt.hidden) {
  fetch(v6_endpoint)
    .then((req) => req.text())
    .then((resp) => {
      ip6.innerText = resp.trim();
      ip6txt.hidden = false;
    })
    .catch((failed) => {
      console.log("Request failed:" + failed);
    });
}
