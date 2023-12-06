import os
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

# print(images.keys())

cache_names = sorted(set(key[2] for key in images.keys()))
policies = sorted(set(key[0] for key in images.keys()))

os.makedirs('plots', exist_ok=True)

print(TRACES)

for cache_name in cache_names:
    im_shape = images[policies[0], next(iter(TRACES)), cache_name].shape
    fig_width = 16
    fig_height = (fig_width/len(TRACES)/im_shape[1])*len(policies)*im_shape[0]
    fig, axs = plt.subplots(len(policies), len(TRACES), figsize=(fig_width, fig_height), squeeze=False)
    for i, policy in enumerate(policies):
        for j, (trace_id, trace_path) in enumerate(TRACES.items()):
            trace_name = trace_path.split('/', 1)[1].split('-', 1)[0]
            im = images[policy, trace_id, cache_name]
            tup = (i, j)
            axs[tup].imshow(im)
            axs[tup].set_xticks(np.array([]))
            axs[tup].set_yticks(np.array([]))
            # axs[tup].set_title(f'{policy} - {TRACES[trace_id].split("-", 1)[0]}')
            if i == 0:
                axs[tup].set_title(trace_name)
            if j == 0:
                axs[tup].set_ylabel(policy)
    fig.tight_layout()
    fig.savefig(f'plots/{cache_name}.png')
