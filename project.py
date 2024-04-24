import os
from runner import Job, get_default_config, TRACES, run_jobs, xarray_tabulate, plot_data
import numpy as np
import copy
from tabulate import tabulate
import xarray as xr
from math import sqrt, floor
import matplotlib.pyplot as plt

def bar_chart(ax, xlabels, classes, data, y_label, x_label, use_log):
    # data: [classes, xlabels]
    x = np.arange(len(xlabels))
    width = 1/(len(classes) + 1)

    for idx, class_name in enumerate(classes):
        rects = ax.bar(x + width * idx, data[idx], width, label=class_name)
        # ax.bar_label(rects, padding=3)

    if use_log:
        ax.set_yscale('log')
    else:
        ax.set_ylim(bottom=0)
    ax.set_ylabel(y_label)
    ax.set_xlabel(x_label)
    ax.set_xticks(x + (width * len(classes)/2))
    ax.set_xticklabels(xlabels, rotation=30, ha="right", rotation_mode="anchor")
    ax.legend(ncols=len(classes))

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

data_dict = {}
for job, result in results.items():
    for cache in result:
        data_dict[(job.job_id.rsplit('-', -1)[1], job.trace[0], cache['name'])] = cache

cache_names = sorted(set(key[2] for key in data_dict.keys()))
policies = sorted(set(key[0] for key in data_dict.keys()))
trace_ids = sorted(TRACES.keys())
trace_names = {id: trace_path.split('/', 1)[1].split('-', 1)[0] for id, trace_path in TRACES.items()}

stats = ["misses", "hits", "miss_rate", "mpki", "reuse", "lifetime", "efficiency"]

data_arr = xr.DataArray(np.zeros((len(cache_names), len(policies), len(trace_names), len(stats))), coords={'cache': cache_names, 'policy': policies, 'trace': trace_ids, 'stat': stats})
for (policy, trace, cache), data_obj in data_dict.items():
    for stat in stats:
        data_arr.loc[cache, policy, trace, stat] = data_obj[stat]

print(data_arr)

os.makedirs('plots', exist_ok=True)

for cache_name in cache_names:
    image = make_square(np.array(data_dict[policies[0], next(iter(TRACES)), cache_name]['efficiency_im']))
    im_shape = image.shape
    fig_width = 16
    fig_height = (fig_width/len(TRACES)/im_shape[1])*len(policies)*im_shape[0]
    fig, axs = plt.subplots(len(policies), len(TRACES), figsize=(fig_width, fig_height), squeeze=False)
    for i, policy in enumerate(policies):
        for j, trace_id in enumerate(trace_ids):
            trace_name = trace_names[trace_id]
            im = make_square(np.array(data_dict[policy, trace_id, cache_name]['efficiency_im']))
            tup = (i, j)
            axs[tup].imshow(im)
            axs[tup].set_xticks(np.array([]))
            axs[tup].set_yticks(np.array([]))
            # axs[tup].set_title(f'{policy} - {TRACES[trace_id].split("-", 1)[0]}')
            if i == 0:
                axs[tup].set_title(trace_name)
            if j == 0:
                axs[tup].set_ylabel(policy)
    fig.suptitle('Cache Efficiency per Way')
    fig.tight_layout()
    fig.savefig(f'plots/{cache_name}.png')

    trace_labels = [trace_names[trace_id] for trace_id in trace_ids]
    for stat_name, stat_label, plot_title in [
        ('miss_rate', 'Miss Rate %', 'Cache Miss Rate'),
        ('efficiency', 'Efficiency %', 'Cache Efficiency'),
        ('lifetime', 'Average Lifetime (cycles)', 'Cache Line Lifetime'),
        ('reuse', 'Average Reuse', 'Cache Line Reuse'),
    ]:
        fig, ax = plt.subplots()
        bar_chart(ax, trace_labels, policies, data_arr.sel(cache=cache_name, stat=stat_name)*(100 if '%' in stat_label else 1), stat_label, 'Trace', '%' not in stat_label)
        ax.set_title(plot_title)
        fig.tight_layout()
        fig.savefig(f'plots/{cache_name}-{stat_name}.png')
