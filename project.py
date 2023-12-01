from runner import Job, get_default_config, TRACES, run_jobs, xarray_tabulate, plot_data
import numpy as np
import copy
from tabulate import tabulate
import xarray as xr
from math import sqrt, floor
import matplotlib.pyplot as plt

def make_square(arr):
    n_elem = arr.size
    square_dim = sqrt(n_elem)
    width = 2**(int(floor(square_dim)).bit_length()-1)
    return arr.reshape((-1, width))

jobs = []

config = get_default_config()
for cache in config['caches']:
    cache['repl'] = 'lru'

db_config = copy.deepcopy(config)
for cache in db_config['caches']:
    cache['repl'] = 'lrudb'

for trace in TRACES.items():
    jobs.append(Job(f'project-lru', copy.deepcopy(config), trace, {}))
    jobs.append(Job(f'project-lrudb', copy.deepcopy(db_config), trace, {}))

results = run_jobs(jobs)

images = {}

for job, result in results.items():
    for cache in result:
        images[(job.job_id.rsplit('-', -1)[1], job.trace[0], cache['name'])] = make_square(np.array(cache['efficiency_im']))

print(images.keys())

for cache_name in ('L1', 'L2', 'LL'):
    fig, axs = plt.subplots(2, len(TRACES), figsize=(16,9))
    for i, kind in enumerate(('lru', 'lrudb')):
        for j, trace_id in enumerate(TRACES.keys()):
            im = images[kind, trace_id, cache_name]
            tup = (i, j)
            axs[tup].imshow(im)
            axs[tup].set_title(f'{kind} - {TRACES[trace_id].split("-", 1)[0]}')
    fig.savefig(f'plots/{cache_name}.png')
