import subprocess
import json
import tempfile
import os
from tqdm import tqdm
from tabulate import tabulate
from tqdm import tqdm
from urllib.request import urlretrieve
from multiprocessing import cpu_count, pool

WARM_INSTR = 100000000
SIM_INSTR =  250000000
N_JOBS = 6

TRACE_URL_ROOT = 'https://dpc3.compas.cs.stonybrook.edu/champsim-traces/speccpu/'

TRACES_FILES = [
    "600.perlbench_s-210B.champsimtrace.xz",
    "401.bzip2-226B.champsimtrace.xz",
    "429.mcf-217B.champsimtrace.xz",
    "450.soplex-247B.champsimtrace.xz",
    "453.povray-252B.champsimtrace.xz",
    "456.hmmer-191B.champsimtrace.xz",
    "464.h264ref-97B.champsimtrace.xz",
    "473.astar-153B.champsimtrace.xz",
    "605.mcf_s-472B.champsimtrace.xz",
    "602.gcc_s-1850B.champsimtrace.xz",
]

def download_traces():
    def reporthook(pbar, count, block_size, total_size):
        if count == 0:
            pbar.reset(total=total_size)
        else:
            progress_size = count * block_size
            pbar.update(progress_size - pbar.n)

    for trace in TRACES_FILES:
        print('Downloading', trace)
        with tqdm(unit_scale=True, unit='B') as pbar:
            urlretrieve(TRACE_URL_ROOT + trace, 'traces/' + trace, lambda *a: reporthook(pbar, *a))

if not os.path.exists('traces/'):
    os.mkdir('traces/')
    answer = input('Download champsim Traces? [y/N] ')
    if answer in ('y', 'Y'):
        download_traces()

if not os.path.exists('results/'):
    os.mkdir('results/')

TRACES = {
    int(filename.split('.', 1)[0]): f'traces/{filename}'
    for filename in TRACES_FILES
}


def get_default_config():
    with open('config.json', 'rt') as file:
        return json.load(file)


class Job:
    def __init__(self, job_id, config, trace, data):
        self.job_id = job_id
        self.config = config
        self.trace = trace
        self.ran = os.path.exists(self.result_path())
        self.data = data
        self.result = None

    def total_id(self):
        return f'{self.job_id}-tr{self.trace[0]}'

    def result_path(self):
        return f'results/{self.total_id()}.json'

    def extra_dir(self):
        return f'extras/{self.total_id()}'

    def run(self):
        os.makedirs(self.extra_dir(), exist_ok=True)
        tqdm.write(f'Running  Job {self.total_id()}')
        args = ['cargo', 'run', '--release', '--', '-w', str(WARM_INSTR), '-i', str(SIM_INSTR), '--json', self.result_path(), '-t', self.trace[1], '--config', json.dumps(self.config)]
        tqdm.write(' '.join(args))
        with open(self.extra_dir() + '/stdout.txt', 'wt') as stdout:
            subprocess.run(args, stdout=stdout, stderr=stdout)
        tqdm.write(f'Finished Job {self.total_id()}')
        self.ran = True

    def get_results(self):
        if self.result is None:
            if not self.ran:
                self.run()
            with open(self.result_path(), 'rt') as file:
                self.result = json.load(file)
        return self.result

def run_jobs(jobs):
    # assert len(set(job.job_id for job in jobs)) == len(jobs), 'Job ID must be unique'
    with pool.ThreadPool(min(cpu_count(), N_JOBS)) as exec:
        for _ in tqdm(exec.imap_unordered(Job.get_results, jobs), total=len(jobs)):
            pass
        return { job: job.get_results() for job in jobs }

def xarray_tabulate(table):
    assert len(table.dims) == 2
    header_type = table.dims[1]
    headers = table.coords[header_type]
    index_type = table.dims[0]
    index = table.coords[index_type]
    return tabulate(table, headers=list(headers.data), showindex=list(index.data))

from matplotlib import pyplot as plt

def plot_data(plot_name_root,metric_name,metric_title,table,plot_num=0):
    x = table.coords[metric_name]
    for trace_num in range(len(table.coords["trace"])):
        #print(trace_num)
        plt.plot(x,table[:,trace_num,plot_num],label=table.coords["trace"][trace_num].values,marker="+")
    plt.xscale('log',base=2)
    y_label = table.coords["statistic"][plot_num].values
    title_str = "{} vs. {}".format(y_label,metric_title)
    plt.title(title_str)
    plt.legend()
    plt.xlabel(metric_title)
    plt.ylabel(y_label)
    file_name = "plots/{}_plot{}.png".format(plot_name_root,plot_num+1)
    plt.savefig(file_name)
    plt.clf()

