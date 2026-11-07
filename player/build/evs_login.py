"""Connexion EVS non interactive pour la CI.

Le nom de compte et le mot de passe sont lus dans l'environnement (secrets
EVS_ACCOUNT_NAME et EVS_PASSWD) et passés à l'API Python de castlabs_evs : ils
n'apparaissent jamais sur une ligne de commande ni dans le journal. Seuls des
jetons d'accès sont écrits dans ~/.config/evs/config.json (runner éphémère).
"""
import os
import sys

from castlabs_evs.account import Account

name = os.environ.get('EVS_ACCOUNT_NAME')
passwd = os.environ.get('EVS_PASSWD')
if not name or not passwd:
    sys.exit('EVS_ACCOUNT_NAME ou EVS_PASSWD absent')
Account(no_ask=True).reauth(account_name=name, passwd=passwd)
print('EVS : connecté')
