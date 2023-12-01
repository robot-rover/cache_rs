from runner import Job, get_default_config, TRACES, run_jobs, xarray_tabulate, plot_data
import numpy as np
import copy
from tabulate import tabulate
import xarray as xr

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