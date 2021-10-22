#!/usr/bin/python3
import flask
import flask_accept


app = flask.Flask(__name__)


@app.route('/')
@flask_accept.accept('*/*')
def ip():
    if flask.request.environ.get('HTTP_X_REAL_IP') is not None:
        IP = flask.request.environ.get('HTTP_X_REAL_IP', flask.request.remote_addr)
    else:
        IP = flask.request.remote_addr
    return f"Your IP is {IP}\n"

@ip.support('application/html', 'text/html')
def ip_html():  # put application's code here
    if flask.request.environ.get('HTTP_X_REAL_IP') is not None:
        IP = flask.request.environ.get('HTTP_X_REAL_IP', flask.request.remote_addr)
    else:
        IP = flask.request.remote_addr
    return f"""<title>Your IP is {IP}</title><h1
            style="
                font-size: 32px;
                font-size: 3vw;
                height: 100%;
                width: 100%;
                display: flex;
                position: fixed;
                align-items: center;
                justify-content: center;
            "
        >Your public IP is {IP}
        </h1>"""


if __name__ == '__main__':
    app.run(port=3333)
