#!/bin/bash

# from https://github.com/PSeitz/lz4_flex/blob/main/benchmarks/download_corpus_bench.sh

mkdir bench_files
cd bench_files || exit
wget https://sun.aei.polsl.pl//~sdeor/corpus/silesia.zip
unzip ./silesia.zip
rm silesia.zip

