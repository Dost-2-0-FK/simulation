# Was bereits funktioniert hat

- wenn die Sozialreformen restriktiv sind, erhoet sich die Produktion in Trusts (zum Beispiel um 10 %)
- es koennen Trusts gebaut werden ohne Financiers
-

# Was noch nicht funktioniert

- humans trusts koennen gebaut werden ->

  fix (Teil 1): production units und trusts duerfen keine schnittmenge an resourcen haben (wird auch validiert bei
  trustbau und in config validiert)

  fix (Teil 2): der resources endpunkt im simulation service gibt nur resources zurueck die nicht in production units
  produziert werden

- finanzier eines trusts: aus dem QR code ausgelesene user id ist in dem format vorname_nachname und in der config
  Nachname, Vorname

- (optional - erfordert anpassungen in auth service, simulation und frontend): beim financier auth request from frontend
  und financier verify request sollte auch der share mitgeschickt werden, damit nicht im nachhinein nach autorisierung
  des financiers der share geaendert werden kann
