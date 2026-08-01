# MUST FIX BUGS

- financier flow: bei bezahlung wird sofort von der zone abgebucht, wenn der financier aber nicht genug money hat, wird
  der trust nicht gebaut, aber trotzdem das geld von der zone abgebucht. -> Fix: simulation: defensiv: erst check von
  allen balances, dann abbuchen.
- financier flow: financiers bezahlen nur Geld, keine resourcen!

# Was bereits funktioniert hat

- wenn die Sozialreformen restriktiv sind, erhoet sich die Produktion in Trusts (zum Beispiel um 10 %)
- es koennen Trusts gebaut werden ohne Financiers
- wenn die zone nicht genug money oder resourcen hat, kann kein trust gebaut werden

# Nice to fix

- humans trusts koennen gebaut werden ->

  fix (Teil 1): production units und trusts duerfen keine schnittmenge an resourcen haben (wird auch validiert bei
  trustbau und in config validiert)

  fix (Teil 2): der resources endpunkt im simulation service gibt nur resources zurueck die nicht in production units
  produziert werden

- (optional - erfordert anpassungen in auth service, simulation und frontend): beim financier auth request from frontend
  und financier verify request sollte auch der share mitgeschickt werden, damit nicht im nachhinein nach autorisierung
  des financiers der share geaendert werden kann
