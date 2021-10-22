import flask

app = flask.Flask(__name__)


@app.route('/')
def ip():  # put application's code here
    if flask.request.remote_addr == '127.0.0.1':
        IP = flask.request.environ.get('HTTP_X_REAL_IP', flask.request.remote_addr)
    else:
        IP = flask.request.remote_addr
    return IP


if __name__ == '__main__':
    app.run()
