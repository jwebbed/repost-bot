#!/bin/sh

dstr=$(date +'%y%m%d')
tar -cvf ./backups/repost.backup.${dstr}.db3.tar.gz --use-compress-program='gzip -9' ./repost.db3
sqlite3 ./repost.db3 'VACUUM;'
tar -cvf ./backups/repost.backup.${dstr}.db3.tar.gz --use-compress-program='gzip -9' ./repost.db3
