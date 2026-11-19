"""Cliente mínimo de la API del motor para las pruebas."""
import json, time, urllib.parse, urllib.request


class Motor:
    def __init__(self, url):
        u = urllib.parse.urlparse(url)
        self.base = f'{u.scheme}://{u.netloc}'
        self.t = urllib.parse.parse_qs(u.query)['t'][0]

    def cruda(self, api, **q):
        q['t'] = self.t
        with urllib.request.urlopen(f'{self.base}/api/{api}?{urllib.parse.urlencode(q)}', timeout=600) as r:
            return r.read()

    def __call__(self, api, **q):
        return json.loads(self.cruda(api, **q))

    def espera(self, seg=600, **cfg):
        t0 = time.time()
        while time.time() - t0 < seg:
            e = self('estado', **cfg)
            if not e['ocupado']:
                return e
            time.sleep(0.2)
        raise TimeoutError('el motor sigue ocupado')
