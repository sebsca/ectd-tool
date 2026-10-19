<!ENTITY % env-countries "at|de|common|ema">
<!ELEMENT eu-envelope (envelope)>
<!ELEMENT envelope (identifier,submission,submission-unit,applicant,agency,procedure,invented-name,inn?,sequence,related-sequence,submission-description)>
<!ATTLIST envelope country (%env-countries;) #REQUIRED>
<!ELEMENT identifier (#PCDATA)>
<!ELEMENT submission (number?,procedure-tracking)>
<!ATTLIST submission type (none|initial|var-type2) #REQUIRED mode (single|grouping) #REQUIRED>
<!ELEMENT number (#PCDATA)>
<!ELEMENT procedure-tracking (number)>
<!ELEMENT submission-unit EMPTY>
<!ATTLIST submission-unit type (initial|response) #REQUIRED>
<!ELEMENT applicant (#PCDATA)>
<!ELEMENT agency EMPTY>
<!ATTLIST agency code (EU-EMA|AT-BASG) #REQUIRED>
<!ELEMENT procedure EMPTY>
<!ATTLIST procedure type (national|centralised) #REQUIRED>
<!ELEMENT invented-name (#PCDATA)>
<!ELEMENT inn (#PCDATA)>
<!ELEMENT sequence (#PCDATA)>
<!ELEMENT related-sequence (#PCDATA)>
<!ELEMENT submission-description (#PCDATA)>
